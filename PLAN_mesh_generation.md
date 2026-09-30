# PLAN v3 (rev 3.4) — mesh generation (`mesh` pipeline)

**Status: plan of record from 2026-09-01, and the only plan document.** **Committed once, on the
owner's instruction of 2026-09-24, and to be destroyed when every subtask in §11 is done** (M-8.3):
the specs, `AGENTS.md` and the docs are the durable record; this file is not. It replaces the previous
`PLAN_mesh_generation.md` (10,490 lines: the rev-2 plan and 66 recorded steps), which was deleted
on 2026-09-01 at the owner's instruction. Everything from it that this plan relies on is carried
here: the findings and numbers it cites in **Appendix A** (keyed by the deleted file's section
numbers, so a citation like *record §6.49* resolves to A-§6.49), and the design content that lived
only there — stage designs S9–S11, the ID semantics not in the specs, the determinism modes, the
GPU/CPU split, the scale envelope, the synthetic test suite, the acceptance-case definitions, the
GPU phases — in **Appendix B**. An implementer needs this document, the three `SPEC_*.md` freezes
and `AGENTS.md`'s meshgen lessons; nothing else.

Written after a review of the record, the three specification freezes, the shipped code at
`0a8eb1c`, and a fresh measurement of all nine acceptance cases and the reference dataset on both
S8 paths (§2). The review's findings are §6; the work is §7.

**Revision 3.1, 2026-09-23 — the 2026-09-11 review is merged here.** `PLAN_MESH_GEN_REVIEW.md`
(fourteen findings MG-01..MG-14, four errata, an implementation order and a reproduction script,
written against `891badc`) is absorbed into this document and retired: its findings are §6.5 and
the subtasks they own (M-1.0, M-1.5, M-1.6, M-2.0, M-4.0, M-4.8, the rewritten M-5.3, M-6.1,
M-6.2, M-6.6 and M-7), its errata are applied in place, its verification record is Appendix A
(`§R`), and its script is Appendix C. The three specification freezes were audited clause by
clause against the code at `891badc` on the same day and revised — geometry rev 1.6, numerics rev
1.3, contracts rev 1.3 — which discharges the *text* half of M-0.1; the code half (the verifier
changes those revisions describe as normative-but-not-built) is M-1.0. `src/meshgen/` is
byte-identical between `0a8eb1c` and `891badc` (`git diff --stat 0a8eb1c..891badc -- src/meshgen`
is empty), so every measurement in §2 stands at both commits. Two owner decisions are open
(§12, D-6, D-7); D-8 was directed on 2026-09-24 toward a cohesive layer (§12.1, M-6.7) and keeps
three sub-decisions open; nothing else in this revision waits on the owner. **Rev 3.2
(2026-09-24)** adds §12.1, M-6.7 and S-52 only.

**Revision 3.3, 2026-09-25 — the owner's requirements of that day**, now **R-E4**, **R-E5**,
**R-E6** and **R-B3** in §4. (1) Resolution is stated as a **background lattice** and a **maximum
refinement level** below it (the reference tool's `*NUM_ELEMENT` and SAMR level), and either may be
given or derived from the geometry: both given, one given and the other derived, or both derived.
(2) The background's hexahedron-to-tetrahedron split shows the **X pattern** — on every axis plane
the diagonals of each 2×2 block of faces meet at its centre — instead of one diagonal direction
everywhere. (3) The two policies derived resolution needs and the geometry cannot supply — how a
thin region is handled, material and gap separately, and whether the far field is capped — are
**user options**, the recommended answers their defaults, every option implemented and tested.
(4) Thin regions are tested **tilted** to the lattice, **irregular** (curved, with an irregular
outline) and **non-uniform** in thickness, for material and for gaps: every thin fixture today is
an axis-aligned box of uniform thickness. They become **M-1.8** (the ladder, given), **M-4.6**
(rewritten: the ladder derived, with the options), **M-1.7** (the X-pattern lattice) and **M-4.9**
(the thin family, fixtures A-17..A-20), and owner decisions **D-9** and **D-10** (§12) were taken
the same day — the reference tool's own 5-tet parity checkerboard, and user options. Checking
the first requirement against the reference dataset found that §2.3's "matched resolution" was not
matched (§2.3's correction note, §6.2 item 18): `run_reference.py` reads the reference log's face
diagonal as its cell edge and the first of several per-section levels, and this mesher's
power-of-two ladder moves the result again — the third case ran at 1.41× the reference's
background and one refinement level where the reference used two. S-53 and S-54 record the two
spec gaps the requirements expose. **Appendix D** holds the two scripts behind every new number.

**Revision 3.4, 2026-09-26 — implementation starts, and it is looked at.** The owner's instruction
on starting M-1: every change is checked **visually as well as by the verifier**, with the
analysis concentrated where the geometry is hard — **intersection curves, sharp edges and
corners**, thin regions and lattice transitions. That is **R9** (§3) and its tool **M-1.9**, a
focus-region render harness that finds those places from the *input* and renders each one
zoomed and in cross-section, before and after, on a fixed camera. M-1.9 lands with M-1.1, before
the first subtask that changes a mesh (M-1.8), so every later change has a "before" to be set
against.

**Rules of this document.**

- **R-N1** applies to every line: the external reference project is cited by role only.
- **Every measured number names the script that reproduces it** (§13). A measured row with no
  runnable source is a claim, not a record — three frozen tables and one plan row have been wrong
  that way (record §2.0, SPEC geometry §15 D-13/D-14).
- **Phase and subtask ids are `M-n.m`**, so nothing collides with the record's `G*`/`P-*` ids. When
  this document cites a record entry it uses the record's own id (e.g. record §6.49), and that id is
  an entry in Appendix A.
- **Owner decisions taken 2026-09-01** (§12): D-1 yes, D-2 no, D-3 yes, D-4 a user config option, D-5 yes.
  None remain open.
- **The standing rule, restated by the owner on 2026-09-01 and binding on every phase (R7):** no hanging
  node, no non-manifold edge, and — unless the user has asked for a crack (D-4) — the whole mesh is
  one continuous conforming complex filling the domain box, whose only feature edges are the box's.
- **The original design goals are re-verified in §2.7** against the shipped code and today's
  measurements; every gap found there has a subtask.
- **MG-nn ids** are the 2026-09-11 review's keys, kept so a citation like *MG-06* resolves to §6.5
  and to the subtask that owns it; **MG errata** (the review's §4) are applied in place and noted
  where they land.
- **Owner decisions D-6 and D-7 are open** (§12); they gate the *definition* of P3/P4, not any
  subtask's start. **D-8 is directed** (2026-09-24): a sub-`t_sheet` gap is delivered as a
  cohesive layer on both walls (§12.1, M-6.7), with D-8a..D-8c open inside it.
- **Owner decisions D-9 and D-10 were taken on 2026-09-25** (§12): D-9 the 5-tet parity
  checkerboard (M-1.7); D-10 user options for both of derived resolution's policies, the
  recommended answers as defaults, every option built and tested (M-4.6).

---

# PART I — the goal and where we stand

## 1. The goal (unchanged)

Generate a tetrahedral volume mesh from **any** STL input, with:

| # | property | normative meaning |
|---|---|---|
| **P1** | any input | any STL set the user supplies: intersecting, touching, nested, sheets, defective. No input class is out of scope and none causes a hard failure |
| **P2** | minimum elements | the fewest elements that satisfy P3 and P4. Element count is a first-class output, not a byproduct |
| **P3** | exact surfaces | every material boundary in the mesh **lies on the input surface**. Not "within `h`", not "bounded chamfer" — on it |
| **P4** | no bad elements | every element is usable by an implicit FEM solver, on the `[V4]` gates |

These are conjunctive. A mesh meeting three of four does not meet the goal.

### 1.1 What P3 forbids (unchanged)

A material boundary is a face between two elements whose region sets differ, or between an element
and the void. **Every such face must lie in the input surface.** A face parallel to the surface and a
fraction of `h` away is a violation, however small the fraction. Visible serration is the symptom;
the displacement is the violation.

### 1.2 What "done" means — the acceptance gate, stated once

The module is complete when, on every case of the acceptance suite — record §17.4's A-1..A-9
including the never-built A-3-ranked, A-4b, A-5 and A-9, plus the ones this plan adds (A-10 forging
contact, A-11 open sheet crossing the box, A-12 non-cubic domain, A-13 defective input, A-14 mixed
single file, A-15 borrowed closure, A-16 fully merged component, and rev 3.3's thin family A-17..A-20
— tilted, irregular, non-uniform; M-4.0, M-4.4, M-4.7, M-4.9) — **and** on the three reference cases
at matched resolution (M-1.8), the **exported** mesh (S11,
not a debug snapshot) satisfies all of:

0. **The mesh is one continuous conforming complex (R7):** `[V3]` reports 0 hanging nodes, 0
   non-manifold edges, 0 boundary leaks, 0 multi-shared faces; every free face lies on the domain
   box; `[V8]` partitions = 1 unless a sheet partitions the box; opened in ParaView, Feature Edges
   shows the box's twelve edges and nothing else — unless the user has asked for a crack (D-4,
   `output.interface: split`), in which case the crack's rim is the only other edge and the two
   sides are the only permitted duplicate nodes. This clause is checked first, on every case, in
   every phase; a change that fails it does not land whatever else it buys (record §6.53).
1. `mesh-verify` reports `fail = 0`, where `[V13]` is a **FAIL** gate at `on_surface_area_frac = 1.0`
   (P3) under the **containment** criterion of M-1.5 — the corner test is its necessary half and stays
   as a diagnostic (MG-02) — `[V4]` a FAIL gate at its §4 defaults (P4, subject to D-7),
   `[V2]`/`[V3]`/`[V6]`/`[V9]` FAIL gates as today, and `[V10]` runs on the INP.
2. Element count is reported per unit input surface area and is not worse than the P2 baseline this
   plan establishes at M-1.4, measured **at equal fidelity** (record §6.50), never at equal `h`.
3. Byte-identical output at `RAYON_NUM_THREADS` 1 and 8 (R-P2), green twice consecutively.
4. Wall time inside the budget M-6.5 sets per case; a change that doubles a case's time is a
   regression whatever else it buys (record §6.31).
5. No behaviour-changing environment variable exists in the mesher or its verifier (R3, scoped as
   §3 states). `grep -rhoE 'RUSTMSPT_[A-Z0-9_]+' src/meshgen src/pipeline/meshgen.rs src/pipeline/mesh_verify.rs src/config/meshgen.rs`
   lists only variables that change what is printed or dumped.
6. **The verifier is trusted.** Every clause above is read off `mesh-verify`, so the gate holds
   only if the verifier's own negative fixtures fail it (R8): the item cap cannot hide a failure
   (MG-01), a final-stage mesh outside the domain box fails `[V3]` (MG-03), a self-declared
   schema-v1 final document missing an **A** array or carrying an out-of-range side element fails
   a contract check (MG-08), and a material boundary running through a body's interior with every
   corner on its surface fails P3 (MG-02). Until M-1.0 and M-1.5 land, every PASS in §2.2 is a
   necessary condition and no clause of this gate is met.

Until then, every phase below reports against the same nine-plus-three matrix (§2.2, §2.3), on the
S8 snapshot, and says which rows moved.

### 1.3 What P3 is stated against, and where the goals conflict (MG-10)

P3 as written names one object, "the input surface". The pipeline handles three, and P3 cannot hold
against all of them at once:

| object | produced by | what P3 can mean against it |
|---|---|---|
| the **original** surface | the STL as read | the user's statement of the geometry; what `[V13]` compares against today |
| the **effective** surface | S0 (weld, repair; decimation once M-4.5 lands, D-1) and S2 (corefinement; patches within `ε` merged — geometry §10, Invariant M1) | the geometry the mesher undertakes to reproduce exactly; differs from the original by the repair/decimation tolerance and by `ε`-merging, both reported |
| the **collapsed** model | S8b's sheet collapse — two walls closer than `t_sheet` become one sheet at the pair's midpoint (geometry §8.1) | a surface on **neither** original wall; the gap's volume is gone and the two bodies now share nodes |

Two conflicts follow, and each needs an owner decision before the subtask that meets it can be
specified (§12, D-6..D-8):

1. **P4's uniform floor is not achievable on every legal input.** A closed polyhedron with a
   preserved edge whose material angle is 1° forces at least one tet in that wedge below any
   uniform dihedral floor above 1°: the dihedrals of the tets sharing that edge sum to the material
   angle, and no refinement, Steiner insertion or smoothing changes the input's own angle. So "any
   STL (P1) + exact surfaces (P3) + every element above one `[V4]` gate (P4)" is not an
   unconditional contract. The answer is one of: a documented, per-element quality exception for
   input-forced angles, reported with the angle that forces it; a per-element gate derived from the
   local input angle; or a declared failure naming the wedge — never a PASS on a gate that cannot be
   met (D-7).
2. **Sheet collapse violates P3 against the original surfaces by construction.** A
   positive-thickness gap collapsed to its midpoint puts a material boundary on neither wall,
   changes "two bodies with a void between" into "two bodies sharing nodes", and removes volume.
   Requiring both "the bodies never weld" (B.9, A-7) and "P3 exact against the input" of an output
   that *is* a welded sheet is contradictory. The decision is which object P3 holds against after
   S8b — the collapsed model, with the collapse reported as a volume and a connectivity change — and
   what "welded" means to the solver (D-8), which is the same question D-4 answered for contact.
   A third representation — the collapse kept as the *meshing* device and the delivered gap
   carried as a cohesive layer whose two faces sit on the two original walls — puts the boundary
   back on both walls and is the owner's direction of 2026-09-24 (§12.1).

P1's "no input class causes a hard failure" also reads against R1's "a cell nothing can mesh is a
hard error with a dump" and `repair: strict`'s refusals. These are three different statements —
which input *classes* the mesher accepts, whether a *specific* input is meshable, and the
conditions for a *delivered* mesh — and P1 is restated to keep them apart: **no input class is
refused by design; a specific input may still be refused, and every refusal is a named, reported
condition (a repair rule, an unmeshable cell, a contract violation), never a silent degradation.**

## 2. Current state, audited 2026-09-01

### 2.1 What is implemented

Stages S0–S8 run end to end (`src/pipeline/meshgen.rs`); **S9 (quality), S10 (regions/partitions
as a stage) and S11 (domain trim, export) do not exist** — the pipeline returns
`NotAvailable("mesh: stages S9..S11 are not implemented yet")` after writing the S8 snapshot, and
`mesh` therefore exits non-zero on every run. `[V10]` and `[V11]` are emitted as `Skipped`
unconditionally; `mesh-verify --compare` does not exist. Two frozen remedies are unimplemented by
recorded decision (`cut.rs:14–23`): K1's refine-in-cut and §4.4's ladder steps 1–2.

**S8 has two paths, and the one that reaches the goal is behind an environment variable.**

| | default path (`RUSTMSPT_PLC_PASS` unset) | gated path (`RUSTMSPT_PLC_PASS=1`) |
|---|---|---|
| single-patch cells | §6's frozen table | §6's table where the trace agrees with §5.2; **§7.2/§7.4's local mesher** where it does not (SPEC §7.1 rev 1.5) |
| junction / escalated cells | §7.6 as built: split by each surface, then a **centroid fan** per piece | the same, after §7.4 has been offered the cell |
| §7.4 refused | — | the **facet-split fan** (record §6.49), then the **whole-cell centroid fan** |
| conformity repair | — | the T-junction detector/repair loop (record §6.57), fixed-point-guarded |
| declaration of the material boundary | `declare_labelled_boundary`, `contact_chamfered_by` (record §6.66) — **both paths** | same |

Four other behaviour-changing handles survive in `src/`: `RUSTMSPT_CDT`, `RUSTMSPT_NO_JCT_CUT`,
`RUSTMSPT_NO_K1_SECOND_CUT`, `RUSTMSPT_NO_MIXED_BY_CENTRE`. Each is an R3 violation with a recorded
reason (record `AGENTS.md` lesson: *"the gate is a statement about the measurement, not a permanent
hedge"*); M-3.2 retires all five.

Code size: `src/meshgen/` is 42,641 lines; `cut.rs` 9,866 of which `cut_lattice` alone is 4,452
lines and contains the whole gated pass; `cdt.rs` 5,940 with 53 inline tests; `cut.rs` has no
inline tests (20 integration tests in `tests/meshgen_cut_tests.rs`); `junction.rs` has none of its
own. 210 tests pass (`cargo test --release`, 10 suites).

**Re-measured 2026-09-23 at `891badc`** (`src/meshgen/` byte-identical to `0a8eb1c`; the working
tree carries uncommitted changes outside it, `git_dirty = true`): `cargo test --release --offline`
— **39 suites, 609 passed, 0 failed, 20 ignored** (the ignored are benchmarks and GPU-hardware tests). The meshgen suites alone: arrange 48, band 22, classify 13, config 15, cut 20, features 9, gapfield 13, lattice 15, quality-gate 1, sizing 26, snap 12, snapshot 11, surface 19, thin 20, mesh-verify 16 — 260 integration tests — plus 135 lib tests across the crate, 53 of them in `cdt.rs`. (v3's "210 tests, 10 suites" was a count over the meshgen suites at `0a8eb1c`; the difference is the placement work merged since, not meshgen.). The five behaviour-changing handles are unchanged (`RUSTMSPT_PLC_PASS`
`cut.rs:1562`, `RUSTMSPT_CDT` `cut.rs:8802`, `RUSTMSPT_NO_JCT_CUT` `cut.rs:8413`,
`RUSTMSPT_NO_K1_SECOND_CUT` `cut.rs:1279`, `RUSTMSPT_NO_MIXED_BY_CENTRE` `junction.rs:382`); the
other 24 names in `src/` are print/dump-only diagnostics, build identity (`build.rs`), or the
shared device/backend selectors `RUSTMSPT_GPU_DEVICE` (`src/gpu/context.rs:94`) and
`RUSTMSPT_ACCELERATION` (`src/compute/policy.rs:141`), which R3 exempts (§3).

**Verifier and identity defects, found by the 2026-09-11 review and re-run at `891badc` on
2026-09-23** (`reproduce.py`, Appendix C; every row's numbers are that run's). These are not S8
defects: three let `mesh-verify` say PASS on a wrong mesh, one lets the P3 gate pass a boundary
through the body, and four are identity and contract errors in S0/S2b/S8 that no acceptance case
reaches. Each is owned by a subtask in Part II; §6.5 carries the full table.

| finding | at `891badc` | what it means for §2.2 |
|---|---|---|
| MG-01 `max_items_per_section: 0` on `bad_inverted_tet.vtu`: `V1 = FAIL`, `summary.fail = 0`, exit **0** | reproduces | a FAIL count is a count of *printed* findings; every `fail = 0` above is conditional on the cap |
| MG-02 `good_cube.vtu` against a `[0,1]³` cube STL: `on_surface_area_frac = 1.0`, `chord_max = 0.5`, `[V13]` PASS | reproduces | `on %` is a necessary condition; a face through the body with its corners on the surface reads 100 |
| MG-03 `good_cube.vtu` with x doubled, `DomainMax = [1,1,1]`, `StageIndex = 11`: `[V3]` PASS, `boundary_leaks = 0` | reproduces | `delivered_ok` (R4/R7) reads a `[V3]` that relaxes to the mesh's own hull at stage 11 |
| MG-08 `constraint_kind`/`constraint_ref` removed; `FaceTagSideElems = [999999, …]`; `ComponentKind[0] = sheet` with three tets — each `fail = warn = 0`, exit 0 | reproduces | a final contract document is not validated for presence or semantic references |
| MG-04 one STL with two disjoint cubes → `ComponentX = [1]`; one STL with a cube and a disjoint open quad → one sheet component, the cube claims no volume | reproduces | R-A2 (X per connected closed component) is per *file* in the code |
| MG-05 a five-face `solid` plus an independent sheet over its opening: `ComponentClosed` `[0,0]` at s00 → `[1,0]` at s02 | reproduces | S2b proves closure with another component's face |
| MG-07 two cubes in contact at `x = 0.4`: 270 tag sets, 280 `FaceTagComponents` members, **270** `FaceTagOrientation` entries | reproduces | the per-member orientation table is per face, and all `+1` |
| MG-09 `a=(0.1,0.1,0.5−2e-8)`, `b=(0.5,0.1,0.5+2e-8)`, `c=(0.1,0.5,0.5+2e-8)`, `p=(0.15,0.15,0.5−1.1e-8)`: f32 `det = −3.58e-9` passes G1's bound `1.55e-15`; exact f64 `det = +1.60e-10` | reproduces (arithmetic; no GPU kernel exists) | numerics §6.1's certificate bounds arithmetic on f32 inputs, not the f64→f32 conversion of the inputs |

### 2.2 The nine acceptance cases, both S8 paths

**Both paths, all nine cases, at `0a8eb1c`, 2026-09-01** (`run_acceptance.py`, §13). `on %` is `[V13]`'s share of material-boundary area anchored to the input surface (P3 is met only at 100); `tets/area` is elements per unit input surface area (R2); gate columns are `mesh-verify` statuses.

| case | path | tets | tets/area | on % | disp | vol err % | `[V1]` | `[V2]` | `[V3]` | `[V4]` | `[V6]` | `[V9]` | `[V13]` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a1 | default | 32,200 | 28,608 | 92.524 | 1.00 | 0.773 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a1 | gated | 59,472 | 52,838 | 99.789 | 0.88 | 0.001 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a2 | default | 111,756 | 64,687 | 100.000 | 1.00 | 0.000 | PASS | PASS | PASS | WARN | PASS | PASS | PASS |
| a2 | gated | 112,117 | 64,896 | 100.000 | 1.00 | 0.000 | PASS | PASS | PASS | WARN | PASS | PASS | PASS |
| a3 | default | 193,495 | 137,831 | 92.784 | 0.71 | 0.512 | PASS | FAIL (19) | PASS | WARN | FAIL (73) | PASS | WARN |
| a3 | gated | 275,083 | 195,949 | 97.205 | 0.31 | 0.021 | PASS | FAIL (3) | PASS | WARN | FAIL (61) | FAIL (18) | WARN |
| a4 | default | 1,745,924 | 1,003,761 | 98.818 | 0.97 | 0.049 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a4 | gated | 1,857,484 | 1,067,898 | 99.095 | 0.85 | 0.038 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a6a | default | 348,254 | 355,347 | 99.539 | 0.91 | 0.273 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a6a | gated | 351,577 | 358,737 | 99.948 | 0.43 | 0.012 | PASS | PASS | PASS | WARN | PASS | FAIL (6) | WARN |
| a6b | default | 277,798 | 279,700 | 99.439 | 0.75 | 0.003 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a6b | gated | 278,361 | 280,267 | 99.961 | 0.79 | 0.004 | PASS | FAIL (1) | PASS | WARN | PASS | FAIL (6) | WARN |
| a7a | default | 405,196 | 212,474 | 99.829 | 0.76 | 0.019 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a7a | gated | 410,086 | 215,038 | 99.964 | 1.00 | 0.002 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a7b | default | 420,468 | 220,482 | 99.892 | 0.87 | 0.019 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a7b | gated | 424,152 | 222,414 | 99.983 | 1.00 | 0.000 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a8 | default | 1,295,621 | 769,006 | 92.038 | 0.82 | 1.579 | PASS | PASS | PASS | WARN | PASS | PASS | WARN |
| a8 | gated | 1,603,211 | 951,573 | 99.264 | 0.47 | 1.295 | PASS | FAIL (6) | PASS | WARN | PASS | FAIL (14) | WARN |

**What the table settles.**

- **The gated path is better on P3 on every case that is not already exact.** Suite off-surface
  area `0.338 → 0.070` (−79 %); a1 92.524 → 99.789 %, a3 92.784 → 97.205 %, a8 92.038 → 99.264 %;
  the four thin cases all above 99.94 %. Displacement share falls on every case where the fan was
  the cause (a3 0.71 → 0.31, a6a 0.91 → 0.43, a8 0.82 → 0.47): what remains is roughness, not a
  shifted boundary.
- **The five never-gated cases pass `[V1]`/`[V3]` on the gated path** — a2, a4, a6b, a7a, a7b — so
  the a8 mechanism (record §6.53) does not recur. Conformity to the mesh is not the open question.
- **The gated path regresses two FAIL gates the default passes.** `[V9]` on a3, a6a, a6b and a8
  (18/6/6/14 curve nodes whose neighbourhood carries only one of the bodies that meet there) and
  `[V2]` on a6b and a8 (1 and 6 duplicate pairs one float32 ULP apart). Both are located (§2.4):
  the `[V9]` nodes are interned nodes in junction cells the kernel refused, and the `[V2]` pairs are
  a lattice-plane/input-plane near-coincidence. Neither is a reason to keep the fan; both are
  reasons M-3 waits for M-2 and M-4.1.
- **Element cost is where the surface is.** Suite 4,830,712 → 5,371,543 (+11.2 %), concentrated
  on a1 (+84.7 %) and a3 (+42.2 %) where a sphere puts a lot of surface inside each cell, and a8
  (+23.7 %); +0.2–1.2 % on the four thin cases; a2 +0.3 %; a4 +6.4 %. Record §6.50 measured the a1 premium against fidelity: the shipped path
  needs `h/2` and 3.6× the elements to reach less than the gated path's on-surface share.
- **`[V4]` is WARN on every row.** Minimum dihedral `4.3e-5°` (a3 default) to `0.29°` (a2 gated),
  worst aspect ratio up to `3.5e12`. Neither path is usable by a solver as delivered. P4 is a phase,
  not a residual (M-5).
- **The thin path fires on none of the four thin fixtures, on either path.** `[V7]` reports 0
  sheet faces and 0 band elements on a6a, a6b, a7a and a7b; a3, which has no thin feature, carries
  514 band elements on the default path. On a7a S3 finds the gap (19 regions: 1 sheet, 0 band, 18
  skipped — 17 `Undersampled`, 1 `MidSurfaceInvalid`), the FEM-aware ladder answers
  `RefineLocally: 1`, and S8b then declines every band it is offered — 312 times on a7a, `FaceShape`;
  on a6a 156 `UncutFaces` and ~800 `FaceShape`. The fixtures were written to test R-B1 and R-C1
  and test neither (M-4.3).
- **The delivered file is the volume on every row** (`delivered_ok`, R4): tets only, region key
  as a cell array, free faces on the domain box only.

### 2.3 The reference dataset at matched resolution, both paths

**Three reference cases at the resolution the reference tool's own log states, both paths, at
`0a8eb1c`, 2026-09-01** (`run_reference.py`, §13). `ratio` is our element count over the
reference's; `fan cells` is `[V12]`'s `junction`-provenance cell count, which on the gated path is
every cell §7.2/§7.4 took; `tets/cell` is over the whole lattice.

| case | path | tets | reference | ratio | on % | disp | fan cells | tets/fan cell | tets/cell |
|---|---|---|---|---|---|---|---|---|---|
| TestCaseIntersect1 (3 surfaces, 2 levels) | default | 149,817 | 354,372 | **0.42×** | 91.02 | 0.93 | 1,452 | 16.8 | 2.00 |
| TestCaseIntersect1 (3 surfaces, 2 levels) | gated | 401,316 | 354,372 | **1.13×** | 98.61 | 0.06 | 16,833 | 20.4 | 5.36 |
| TestCaseIntersect2 (2 surfaces, 2 levels) | default | 331,068 | 730,199 | **0.45×** | 94.73 | 0.93 | 2,611 | 18.0 | 1.68 |
| TestCaseIntersect2 (2 surfaces, 2 levels) | gated | 653,523 | 730,199 | **0.89×** | 98.72 | 0.12 | 28,628 | 16.9 | 3.32 |
| TestCaseIntersect3 (9 surfaces, 1 level) | default | 459,341 | 1,167,239 | **0.39×** | 94.96 | 0.69 | 2,896 | 17.3 | 1.80 |
| TestCaseIntersect3 (9 surfaces, 1 level) | gated | 784,843 | 1,167,239 | **0.67×** | 99.37 | 0.14 | 47,094 | 12.2 | 3.08 |

**What this table settles, and it is the one number in this document that moves against the plan.**

- **P3 on real geometry follows the fixtures.** On-surface share 91.0 → 98.6 %, 94.7 → 98.7 %,
  95.0 → 99.4 %; displacement share 0.93 → 0.06, 0.93 → 0.12, 0.69 → 0.14. The gated path's residue
  on the reference cases is roughness at the input's own tessellation, not a shifted boundary.
- **The element ratio against the reference is no longer 0.4×.** At matched resolution the gated
  path emits **1.13× / 0.89× / 0.67×** the reference's count against the shipped path's 0.42× /
  0.45× / 0.39×, and on the largest case it is more elements than the reference. The lattice is
  identical (74,930 / 197,000 / 255,062 cells), so the whole difference is the traced cells:
  16,833 cells at 20.4 tets each on the first case, where the shipped path fanned 1,452 at 16.8. The
  kernel is not wasting them — record §6.50 showed 16.9 tets against a 20-triangle augmented
  boundary is cheaper than a fan of it — the boundary is the input's tessellation, which these STLs
  carry at or below `h`. **Under P3 as stated, the element count of an exact-conforming mesh is
  bounded below by the input's own vertex density**, and that bound is what the reference cases
  meet. The reference tool's count is lower because it chords, which is the same 5–9 % of surface
  area the shipped path leaves off.
- Consequently R2's comparison is **at equal fidelity or not at all** (M-1.4). The owner has
  decided (D-1, §12) that the input **may** be decimated within a stated tolerance *before* meshing,
  as input conditioning the user chooses per geometry (M-4.5); that is what will bring the reference
  cases' counts down, and it changes the input, not the cut. Nothing in this plan tunes the mesher
  toward a chording count.

> **Correction (rev 3.3, 2026-09-25): the resolution in this table is not the reference's.**
> `run_reference.py` sets `h_max` to the log's `Base Mesh Element size` and halves it once per the
> first `SAMR levels:` line. Read against the reference tool's own inputs (`*RVE_CORNERS`,
> `*NUM_ELEMENT`, each section's `*SAMR_LEVEL`), neither is what the reference used. (i) The logged
> size is the background cell's **face diagonal**, `√2 ×` its edge, on all three cases to the
> printed digit (37.4351 = √2·450/17; 0.0235702 = √2·0.4/24; 0.0868504 = √2·1.35107/22). (ii) The
> third case's first `SAMR levels:` line reads `0` while eight of its nine sections are at level 2
> (the script's own comment, that raising 0 to 1 "biases the comparison against this mesher", is
> backwards there). (iii) This mesher's octree rounds both bounds down onto a power-of-two ladder
> anchored at the longest axis (S-54), which cannot express a 17-, 24-, 30-, 22- or 45-cell
> background at all. What the rows above actually ran at (`reference_resolution.py`, Appendix D.2):
>
> | case | reference background, levels | realised here | background edge vs reference | finest edge vs reference |
> |---|---|---|---|---|
> | TestCaseIntersect1 | 17³ cells of 26.4706, level 2 | 16³, 2 levels | 1.0625× | 1.0625× |
> | TestCaseIntersect2 | 24×24×30 of 0.0166667, level 2 | 26×26×32, 2 levels | 0.9375× | 0.9375× |
> | TestCaseIntersect3 | 22×45×45 of 0.0614125, level 2 on eight sections, 0 on one | 16×32×32, 1 level | **1.4062×** | **2.8125×** |
>
> So the ratio column compares meshes of different resolution: in uniform regions the realised
> lattice has 0.83× and 1.21× the matched cell count on the first two cases (the edge ratio cubed),
> and the third case's mesh is one refinement level and 1.41× background coarser than the
> reference's. The on-surface and displacement columns are properties of this mesher's own mesh
> and stand; the element ratios, A-10's reading and §2.5's P2 verdict do not, until **M-1.8**
> re-runs the three cases at the reference's own background and per-section levels — which needs
> arbitrary background counts and per-input levels, i.e. the owner's resolution requirement
> (R-E4). The rows above are kept as measured and labelled; they are not a baseline. Record §2.0's
> 0.40×/0.47×/0.41×, the correction this plan inherited as P2's standing number, used the same
> mapping.

### 2.4 Where the remaining defects are, charged to the arm that made them

`RUSTMSPT_PLC_PASS=1 RUSTMSPT_CUT_DIAG=1 RUSTMSPT_PLC_DIAG=1`, the `[PLC]` census lines and the
`plc_path` cell array (§13). Arms: **§6's table** (the trace agrees with §5.2), **§7.4-meshed**
(the local mesher), **facet-split fan** (§7.4 refused; the cell split by its own surface triangles
and each piece tetrahedralised or fanned), **whole-cell fan** (the split could not be built either).

| | a3 | a6a | a8 |
|---|---|---|---|
| traced cells offered to §7.4 | 17,111 of 121,764 | 36,257 of 203,628 | 132,177 of 924,636 |
| §7.4-meshed / facet-split fan / whole-cell fan (cells) | 15,470 / 1,309 / **332** | 35,803 / 379 / **75** | 128,036 / 860 / **3,281** |
| tets per cell by arm | 8.24 / 26.85 / 20.28 | 4.89 / 12.70 / 15.31 | 5.85 / 17.65 / 13.76 |
| off-surface area carried by the whole-cell fan | **91.7 %** of the case's total (89.0 % of its own area is off) | **94.6 %** (91.3 %) | **98.4 %** (17.7 %) |
| off-surface area carried by §7.4-meshed | 7.0 % (0.2 % of its own) | 3.3 % (0.0 %) | 1.6 % (0.0 %) |
| off-surface area carried by §6's table | 0.4 % (0.2 %) | 0.0 % (0.0 %) | 0.0 % (0.0 %) |
| §7.4 refusals, by reason | 227 facet interior not covered · 87 facet edge not an edge · 13 neither · 2 link polygon · 2 + 1 hull | 42 · 10 · 0 · 1 · 21 hull carries an unknown node · 1 | 1,352 facet edge not an edge · 411 facet interior not covered · 1,200 hull carries an unknown node · 228 boundary node off the hull · 60 link polygon · 20 tet thinner than the quantum · 8 neither · 2 other |
| P3 stranded by refusal (share of stranded area) | 81.7 % facet interior · 12.4 % facet edge · 4.7 % neither | 44.4 % facet interior · 36.1 % hull-unknown-node · 16.4 % facet edge | 38.3 % hull-unknown-node · 26.3 % facet interior · 24.1 % facet edge · 7.9 % link polygon |
| `[V6]` two-component steps: inside one cell / across a lattice face, by arm | 35 (25 whole-cell fan, 10 facet-split) / 17 | 254 (105 / 149) / 66 — all excused by contact (`[V6]` PASS) | 0 / 0 (`[V6]` PASS) |
| `[V9]` failing nodes' incident tets by arm (whole-cell / facet-split / meshed) | 34 / 100 / 52 | 28 / 30 / 8 | 108 incident tets over 12 nodes, all `junction` provenance; by arm not yet measured (M-1.1) |
| tets below 10° by arm (share of the arm) | table 0.07 % · meshed **14.61 %** · whole-cell fan 25.56 % · facet-split 31.32 % | 0.54 % · 7.39 % · 25.26 % · 22.75 % | 0.03 % · **9.16 %** · 20.53 % · 29.10 % |
| of those, inheriting a needle face | 98.6 % · 96.1 % · 93.0 % · 84.9 % | 100 % · 98.9 % · 92.4 % · 82.8 % | 100 % · 97.9 % · 89.1 % · 90.6 % |

Three statements this table makes, each the premise of a phase:

1. **The whole-cell fan is the residual P3 defect.** 332 cells of 121,764 carry 91.7 % of a3's
   off-surface area; 75 of 203,628 carry 94.6 % of a6a's; 3,281 of 924,636 carry 98.4 % of a8's. §7.4-meshed and §6's table are exact to
   0.2 % of their own area. M-2 exists to make those cells meshable and then to remove the arm.
2. **The refusals are boundary recovery, and the stranded area ranks them.** "A facet's edges are
   all there but its interior is not covered" strands 81.7 % on a3, 44.4 % on a6a and 26.3 % on a8;
   "a facet edge is not an edge of the tetrahedralisation" 12.4 %, 16.4 % and 24.1 %. On a8 a third
   class is as large — "the hull carries a node the boundary has never heard of", 1,200 cells and
   38.3 % of its stranded area — and it is not a facet problem: a facet vertex is landing on the
   cell's boundary without being a trace point of that face (the "cut derived twice" family, record
   §6.43, which took this class from 72 % of the population to what is left). Recovery today is flips and edge
   removal only, and 90.4 % of `remove_edge`'s refusals are "the link polygon has no valid
   triangulation" (record §6.49). The remedy is a Steiner point on the facet (M-2.1); the cone in
   the link is measured inert (X-7).
3. **Quality is two-dimensional first.** On every arm of both cases, 83–100 % of the tets below 10°
   inherit a needle face — a triangle that is already bad in the face triangulation J1 froze. The
   kernel's own arm is the largest producer by count (a3 18,627 of 31,427; a8 68,598 of 82,539).
   M-5 starts in the face.

### 2.5 Verdict per goal property

| property | verdict | evidence |
|---|---|---|
| **P1** any input | **untested beyond the fixtures** | nine synthetic cases and three reference cases run; A-3-ranked, A-4b, A-5, A-9 were never built; the real datasets under `workspace/data` have never been fed to the mesher; the thin fixtures do not exercise the thin path (§2.2) |
| **P2** minimum elements | **provisional** | 0.40×–0.47× the reference at matched resolution on the shipped path (record §2.0); on the gated path the same three cases read **1.13× / 0.89× / 0.67×** at 98.6–99.4 % on-surface (§2.3), and the difference is the input's own tessellation carried exactly. **Neither comparison was at the reference's resolution** (§2.3's rev 3.3 correction); M-1.8 re-measures. Not a result until P3 and P4 hold |
| **P3** exact surfaces | **fails on 8 of 9 on both paths; the gated path is within 0.02–2.8 % on eight and exact on a2** — and every one of these numbers is a *necessary* condition only: the corner test passes a boundary that runs through the body (MG-02) | §2.2; the residue is one arm (§2.4); M-1.5 re-measures under the containment criterion |
| **P4** no bad elements | **fails on 9 of 9 on both paths** | `[V4]` WARN everywhere; 83–100 % of bad tets are two-dimensional in origin (§2.4) |
| **deliverable** | **not delivered** | `mesh` exits non-zero after S8; no INP; no domain trim; `[V10]`/`[V11]` skipped (§2.1); the verifier that reports every row above is itself untrusted on three counts (MG-01, MG-03, MG-08 — §2.1) |

### 2.6 Determinism and time

- **R-P2 on the gated path, a1:** `RAYON_NUM_THREADS` 1, 8 and 8 again give identical SHA-256 for
  both `a1_rp2_s08_cut.vtu` (`55a8f89e…`) and `a1_rp2_s08_cut_contract.vtu` (`80fb2476…`). One case;
  M-1.2 does the matrix.
- **Time (`RUSTMSPT_TIME_STAGES`, this host, under concurrent load — indicative, M-1.3 records the
  clean numbers):** a3 gated S8 `16.5 s`, everything before it `< 1 s`; a6a gated S8 `17.1 s`;
  a8 gated S8 `164 s`; a3 default S8 `2.0 s` — the gated path costs **8×** the default on a3. S8 is
  the whole budget, and on the gated path it is the trace, the per-face constrained triangulation and
  the per-cell kernel. The record's one cost
  refutation (a6a 4 s → 10 min, X-5) is the scale a change must not reach.

### 2.7 The original design goals, revisited

No rev-1 plan survives on disk or in git; the record's requirements register (record §1) is
"condensed from rev 1" with every later addition marked, so it *is* the original goal statement.
The owner's five headings (2026-09-01) are checked against it, the shipped code and today's
measurements. **Built** is what exists; **verified** is what a number says today; **gap** is what
no number says or a number contradicts; every gap has an owner.

| # | goal | requirement ids | built | verified today | gap | owner |
|---|---|---|---|---|---|---|
| G1 | **Automatic SAMR** — resolution from the geometry, no hanging nodes | R-E1, R-E2; record §10.6–§10.7, §4 ("SAMR replaced by balanced octree + conforming templates") | S4 sizing field from five criteria (curvature/chord, feature, local feature size, gap `t/gap_cells`, curve `curve_cells`), Lipschitz-graded; S5 strongly 2:1-balanced Morton octree, Freudenthal 6-tet + centroid-fan transitions, conforming by construction (SPEC geometry §2–§3, theorem T1); K1 refine-retry loop (≤ 3 passes); the reference-verbatim hanging-node SAMR fallback (assumption G-5) never needed | `[V3]` 0 hanging nodes on all nine cases, both paths (§2.2); the S5 lattice `[V3]`-clean at 1.65 M tets (record §0.1); S3↔S4 coupling converges (a7a: 2 iterations) | **Not automatic at the bounds.** `h_max`/`h_min` are user fractions and three of nine cases need per-case values to mesh their own geometry: A-4 `h_max` 0.0125 (at 0.05 the cube is 2.7 cells across and reads 25.5 % short; 0.025 → 5.79 %; 0.0125 → 0.82 %), A-6 0.04/0.004 to put the limb in its regime, A-8 `h_min` 0.006 (0.012 → 82.4 % on-surface; 0.006 → 92.0 %; 0.003 breaks `[V1]`/`[V3]`) — `run_acceptance.py` carries them. Refinement requests past the floor pass through (a8: 2,269 open after 3 passes, record §6.16) and are exhausted for sub-cell bodies (X-2). Gate G4-3's post-snap transition quality was never measured. **The owner's requirements of 2026-09-25 sharpen both halves**: the bounds are stated as a background and a maximum level, each given or derived (R-E4) — today the background can only be a power-of-two count along the longest axis (`sizing.rs:1417–1430`), so a reference-style 17-cell or 24×24×30 background cannot be stated at all (S-54); and the split shows the X pattern instead of one diagonal direction everywhere (R-E5) | M-1.7, M-1.8, M-4.6, M-5.1 |
| G2 | **Automatic sharp-edge detection and preservation** | R-A5; record §10.2, §10.9 | S1 dihedral detection at `feature_angle_deg` (45°, algebraic `cos²`), rims and non-manifold edges always features, chained into polylines with corners at junctions and high-turn vertices; S7 corner and feature-curve capture; curves in the arrangement table; on the gated path the trace carries the curve through every traced cell | a2 (cube): 12 curves declared, **12 carried**, 100 % on-surface, `[V13]` PASS; a3: curves carried 40 → **96** of 114 (default → gated); a8: 170 → **393** of 972; `[V9]` radial-patch and fan-closure clauses 0 on all nine, both paths | Carriage is INFO, not a gate (S-8). Contracts §5's feature-curve conformance (≤ 10 % `h`) and sharp-corner error (≤ 1 % `h`) clauses are **not implemented** in `[V5]` (S-12). Record §10.2's "explicit point-list overrides accepted" is **not in the code** (S-13). a4's serrated lip at the cube edges (record §6.49) is reduced, not gone: 0.905 % off-surface on the gated path, 60 % of it near a curve | M-0.1 (S-12, S-13), M-0.2 (S-8), M-2 |
| G3 | **Automatic intersection curves, mesh conforming to them** | R-A5, R-C3; record §10.3–§10.4, topic D | S2 corefinement with the exact intersection registry (C1/C2/C3 staged precision, DD escalation), coplanar overlay, coincidence policy, radial patch order per curve, S2b topology rebuild with GWN fallback; the curve table (`CurveComp`, `CurveRadialPatches`); `[V9]`'s three clauses | Every case and all three reference cases arrange (3, 2 and 9 surfaces) with no residual; `[V9]` radial mismatches **0** and open fans **0** everywhere; a3's intersection curve carried 96 of 114 on the gated path | 18 a3 nodes on the curve have only one body around them (§2.4 — the refused junction cells, M-2); carriage is not a gate; the ranked-priority form of a3 (R-A4: the lens to the winner) has never been run | M-2, M-0.2, M-4.4 |
| G4 | **Forging: opposing surfaces that approach or touch** — a small gap kept as one thin element layer; full contact as a marked interface layer embedded in the mesh | R-B1, R-B2; record §10.5, §10.11, §10.13, topics E/F/G | S3 gap field (two-sided rays, closest-pair sweep, densification, five-check pairing battery, confidence 0.9), hysteresis regimes locked one way; S8b band templates `k = 0..3` with the FEM-aware five-outcome ladder; welded-sheet collapse with rim curves (G7-2); exact contact declared for both bodies (`declare_contact_components`, a6a: 713 faces); `FaceTagSideElems` (elem⁺, elem⁻) reserved on every tagged face for cohesive/split-node treatment | **The machinery detects and then declines.** a7a: S3 finds 19 regions (1 sheet, 18 skipped — 17 `Undersampled`, 1 `MidSurfaceInvalid`), the ladder answers `RefineLocally: 1`, S8b declines all **312** bands (`FaceShape`); a6a: ~800 `FaceShape` + 156 `UncutFaces`. **0 sheet faces and 0 band elements on all four thin fixtures, both paths**; a3, with no thin feature, carries 514 band elements at the lens tip. a6a's contact plane is declared for both bodies where the kernel meshes it and excused as chamfered contact on 163 faces (§2.2) | No case has ever produced a band layer or a collapsed sheet; the forging shape — curved opposing surfaces whose gap closes to contact, spanning volumetric → band → sheet → contact in one part — has no fixture; `collapse_sheets` defaults to `false` and is a correctness knob (R3); whether a contact is exported welded or as a crack is the **user's** choice per run (D-4, `output.interface`); a sub-`t_sheet` gap delivered as a cohesive layer on both walls (§12.1) has no fixture and no export; **every thin fixture is an axis-aligned box of uniform thickness** (`generate_acceptance_cases.py:96–104`) — S3's unit tests measure an oblique and a curved gap (`tests/meshgen_gapfield_tests.rs:248`, `:287`), but nothing tilted, curved or tapered has ever been carried through S8b to a mesh, so a thin path that worked only on lattice-aligned slabs would pass the whole suite (R-B3, owner 2026-09-25) | M-4.3, M-4.7, M-4.9, M-6.6, M-6.7 |
| G5 | **Everything else in the original register** | | | | | |
| G5a | multi-ID semantics `{X, Y}` | R-A1–R-A4 | `resolve()`, region keys, `N_ID`, priority tables (SPEC geometry §9) | `[V6]` PASS on 8 of 9 (a3 FAIL, M-2); same-priority overlap keys on a3/a4 | **every input in every case has priority 0**, so R-A4 (higher replaces lower) is untested — A-3-ranked and A-4b were never built | M-4.4 |
| G5b | open thin sheets, sheet–sheet intersections | R-C1–R-C3 | sheet role, welded sheet cuts, rims (G6-5, G7-2), `[V7]`/`[V8]` clauses | **nothing** — every acceptance case is closed solids (`[G2-4] … 0 sheet` on all), so the sheet path has never run on an acceptance case | no open-sheet fixture exists | M-4.4 (A-11) |
| G5c | domain box: trim, partitions | R-D1, R-D2 | `[V8]` partition flood fill; the trim is S11's and **unbuilt** | `[V8]` partitions = 1 on every case; every domain is `[0,1]³` | neither the trim nor a partitioned domain has ever been exercised | M-6.2, M-4.4 (A-11, A-12) |
| G5d | conforming FEM mesh; direct export | R-E2, R-E3 | `[V1]`–`[V3]`; INP export unbuilt | `[V1]`/`[V3]` PASS on all nine both paths; `[V2]` fails on a3 (both paths), a6b and a8 (gated) | no INP; `mesh` exits non-zero (§2.1) | M-4.1, M-6.2 |
| G5e | input repair | topic M; record §10.1 | `repair: strict \| conservative \| permissive` with the action log and `s00` snapshot; 19 unit tests | unit level only | no acceptance case is defective, so the levels have never been exercised end to end | M-4.4 (A-13) |
| G5f | one framework, GPU-accelerated | R-F1 | CPU path complete to S8; GPU kernels for S2/S3/S6 not started; determinism contract designed | R-P1 (rayon on every stage); R-P2 on a1 | GPU deferred to M-7 by design | M-7 |
| G5g | tooling: VTU contract, verifier, renderer | R-T1–R-T3 | built; `[V10]`/`[V11]` and `--compare` missing | `mesh-verify` on every case; renders in the record | `[V10]`, `[V11]`, `--compare` | M-6.2, M-6.3 |

Two observations across the table. **The mechanisms exist and the fixtures do not**: G4, G5a (priorities), G5b, G5c and G5e all have code that no acceptance case reaches, which is how a
built feature and a working feature came to be confused. M-4.4 and M-4.7 exist to end that.
And **automation stops at the bounds**: the sizing criteria are automatic, the bounds they work
inside are not, and three fixtures needed a human to set them — G1 is not met until they are
stated as the owner states them, a background and a maximum level (M-1.8), and derived from the
geometry whenever the user leaves either one open (M-4.6).

---

## 3. Architecture requirements

R1–R5 are carried from the previous plan unchanged in meaning; R6 is new and is the memory of four
wrong measured rows; R7 is the owner's standing rule, stated in every session since P-3.13 and
written here so no phase can forget it; R8 (rev 3.1) is the 2026-09-11 review's lesson, which the
record had already learned five times about individual checks and never stated as a rule.

### R1 — No fallback may abandon conformity to the interface

Every cell the surface passes through is subdivided so the surface is a **union of element faces**.
A cell the kernel cannot mesh is a gap in the kernel or in a preceding stage and is closed there —
never diverted to a construction that gives up the defining property. **Consequence this plan draws
for the first time:** the whole-cell centroid fan is not a fallback, it is a defect with a name, and
M-2 deletes it rather than declaring around it. A cell nothing can mesh is a hard error with a dump,
because a wrong mesh delivered quietly is worse than no mesh delivered loudly.

### R2 — Element count is a gate, not an outcome

Reported per unit **input** surface area; compared across tools only at a stated, reproduced
resolution (`run_reference.py`); compared across our own changes only **at equal fidelity** (record
§6.50: at equal `h` the gated path costs +93 % on a1; at equal on-surface share the shipped path
needs 3.6× the elements for less fidelity). A change raising the count without a P3/P4
justification is a regression.

### R3 — Features adapt; there are no correctness knobs

There is no setting choosing between a correct mesh and an incorrect one. Reaching for a behaviour
flag is a diagnosis: the stage lacks its adaptive criterion, or the architecture cannot reach the
goal. Diagnostic env vars that change only what is *printed or dumped* are exempt. A
prototype-gate handle is allowed for the life of its gate and **must not survive it** — five have
(§2.1).

**Scope of the grep (MG errata, 2026-09-11).** §1.2 clause 5 is scoped to the mesher and its
verifier — `src/meshgen/`, `src/pipeline/meshgen.rs`, `src/pipeline/mesh_verify.rs`,
`src/config/meshgen.rs`. Shared infrastructure selectors such as `RUSTMSPT_GPU_DEVICE` and
`RUSTMSPT_ACCELERATION` (`src/gpu/context.rs`, the compute policy) choose a device or a backend,
not between a correct and an incorrect mesh, and are outside it; so are the build-identity
variables `build.rs` sets. The census in §13 lists every name with its class.

### R4 — The delivered mesh is the volume

A tets-only unstructured grid carrying region identity as a cell array (`<name>.vtu`), with the
face-tag contract in `<name>_contract.vtu` beside it. Landed (record P-2). S11 adds the INP.

### R5 — Acceptance is measured on the surface, not on proxies

`[V13]` is the measure of P3 and the only one that gates it. `[V5]`, `[V6]`'s undeclared-face
count, cells taken, refusals converted and tets-per-cell are diagnostics: they may inform, they may
not gate (record §6.45: a proxy improved monotonically for four steps while P3 degraded
monotonically).

### R6 — A measured row has a runnable source, and is verified twice before it is written

Every number in this document and in any `SPEC_*.md` table names the script and inputs that
reproduce it (§13). A row that corrects a frozen table is confirmed two independent ways — an
exhaustive enumeration *and* the shipped verifier on real data — before the table is edited, and the
edit carries a `§15` deviation row and a `§14` verification record. The record has four rows that
failed this (record §2.0; SPEC geometry D-13, D-14).

### R7 — One continuous conforming complex, no hanging node, no non-manifold edge

The whole mesh is one part: every interior face has exactly two tets, no node lies on a face or
edge it is not a vertex of, no edge is non-manifold, no face is shared by three tets, every free
face is on the domain box, and the tets fill the box. Extracting feature edges the way ParaView
does must show the box's edges and nothing else. The only exception is a crack the **user** has
asked for (D-4, `output.interface: split`): then the interface's two sides are the only duplicate
nodes and its rim the only other feature edge, and everything else above still holds on each side.

This is `[V3]` plus the box test `run_acceptance.py` already makes (`delivered_ok`), and it is met
today on all nine cases on both paths (§2.2). It is not a phase goal; it is the precondition of
every phase, checked first, and the a8 lesson is its history: a change that improved fidelity by
38 % and left 113,859 hanging nodes was not an improvement (record §6.53). `[V3]`'s tolerances
(`hanging_tol_frac`, `plane_tol_frac`) are frozen at M-0.1 so the rule has one meaning.

### R8 — A check is trusted only after its negative fixture fails it

Every gate in §1.2 is read off `mesh-verify`. A check whose negative case has never been shown to
fail measures nothing: the record found five such checks passing on stub arrays (`regime`,
`FaceTagKind`, `n_id_key`, `CurveComp*`, `constraint_kind`), and the review found three ways for the
whole report to read PASS on a wrong mesh (MG-01, MG-03, MG-08) and one P3 gate that passes a
boundary running through the body (MG-02). Consequences: (i) every check that gates §1.2 has a
committed fixture that fails it, in contracts §6's manifest, before its PASS is cited anywhere in
this document; (ii) a verifier's exit status is computed from every finding, never from the
findings it chose to print; (iii) **frozen is a statement about change control, not about
correctness** — a frozen clause an audit shows to be insufficient is amended with a §15/§11/§9
row, not defended by the freeze, and "the implementation matches the frozen text" closes no
finding that says the text is wrong.

### R9 — Every change to the mesh is looked at where the geometry is hard (owner, 2026-09-26)

Aggregates average away exactly the populations this project keeps finding: the limb's serration
was seen by eye before any metric could name it, a pixel diff on a fixed camera caught in one pass
what three verifier metrics missed (X-3, record §9837), and a user looking at a picture separated
a3's chamfered cube edge from a1's benign chord error. So:

1. **A subtask that changes what S5–S11 emit is accepted only with M-1.9's focus-region render
   set, before and after, looked at and described in its report** — region by region, whether the
   material boundary follows the curve or edge (an element edge lying along it), and any chamfer,
   serration, notch, sliver, stray colour or gap seen. A subtask that should not change the mesh
   shows the set unchanged (changed-pixel share 0). This overrides the **Multimodal: no** flag of
   every mesh-changing subtask in §7 and §11.
2. **The focus regions come from the input, never from the mesh** — a mesh declares only the
   curves it kept (the record's 24 of 1,404 on a8), so regions read off the output hide what was
   lost. They are: (a) **intersection curves** between components (`CurveKind = 2` in the s02
   arranged snapshot: a3's cube–sphere curve, a6a/a6b's limb-root rim, a8's strut junctions, the
   reference cases' particle contacts); (b) **sharp edges and corners** of each input
   (`CurveKind = 0`: every cube edge and corner, the limb's edges, a8's strut creases); (c) **thin
   regions** (S3's regions: a6's limb, a7's gaps, A-17..A-20); (d) **lattice transitions** (level
   jumps in s05), for M-1.7 and M-1.8.
3. **Each region is shown two ways**: a zoomed window a few elements wide around a sample point,
   and a cross-section clip perpendicular to the curve through it, wireframe on, coloured by
   `region_key`, interface faces off, the input curve overlaid — plus the case overview. Fixed
   cameras and fixed sample points (deterministic, from arc length), so two runs compare pixel for
   pixel.
4. **Metrics and pictures must agree, and a disagreement is itself a finding.** A render that shows
   a defect the verifier passes is a verifier gap (R8); a metric that moves with no visible change
   in any focus region says which region set is missing.

## 4. Requirements register (carried forward, still normative)

| ID | Requirement |
|---|---|
| R-A1 | Background/matrix ID `BG_ID = 0` |
| R-A2 | Each connected closed component of a watertight STL gets `{X, Y}`: X unique component ID, Y priority, smaller Y = higher priority — **as built, X is per input file** (MG-04, M-4.0a) |
| R-A3 | Regions may carry multiple IDs (same-priority overlaps preserve all X) |
| R-A4 | Across priorities, higher replaces lower in the overlap; final volumetric regions are non-overlapping across priority levels |
| R-A5 | STL–STL intersections and per-surface sharp features represented explicitly and conformingly |
| R-B1 | Near-contact: preserve the gap with one layer of thin volume elements while resolvable, else collapse to an embedded sheet |
| R-B2 | The band/sheet decision is robust and locally adaptive, tied to the local size field and FEM usability |
| R-B3 | **(owner, 2026-09-25)** Thin regions are tested **tilted** to the lattice (at generic angles, not only lattice-aligned), **irregular** (a curved mid-surface, an irregular non-convex outline) and **non-uniform** in thickness (tapered and rippled, crossing regime boundaries inside one body), for thin material and for thin gaps, through S8b to the delivered mesh — M-4.9, fixtures A-17..A-20 |
| R-C1 | Open thin sheet: no volume elements inside it; surrounding volume conforms to it including its rim |
| R-C2 | Sheet nodes get IDs consistent with volumetric-component logic |
| R-C3 | Sheets may intersect sheets/closed surfaces; intersection curves explicit; curve nodes may carry multiple IDs |
| R-D1 | Mesh only inside the prescribed domain box — **the box becomes an S8 constraint and S11 deletes whole outside cells (M-6.2, MG-13); not yet built** |
| R-D2 | Open sheets crossing the box may partition it; disconnected regions identified and assigned IDs consistently |
| R-E1 | Resolution automatic from geometry — subject to R2: only refinement the cut can represent; stated through R-E4's ladder (rev 3.3) |
| R-E2 | Output is a conforming FEM mesh: no hanging nodes, no nonconforming interfaces, no duplicate coincident nodes, no invalid connectivity, no cracks |
| R-E3 | Directly exportable to conventional FEM solvers, no downstream repair stage |
| R-E4 | **(owner, 2026-09-25)** Resolution is stated as a **background lattice** (cells per axis, or a cell size) and a **maximum refinement level** below it. Each may be given or derived from the geometry — both given, the background given and the level derived, the level given and the background derived, or both derived. A given value is honoured exactly and never silently changed; a derived one is reported with the source that set it. An input may carry its own, shallower level (the reference tool's per-section level) — M-1.8, M-4.6 |
| R-E5 | **(owner, 2026-09-25)** The background's hexahedron-to-tetrahedron split has no preferred direction: on every axis plane the face diagonals form the **X pattern** — the diagonals of each 2×2 block of faces meet at its centre — never one diagonal direction everywhere — M-1.7 |
| R-E6 | **(owner, 2026-09-25)** The policies derived resolution needs and the geometry cannot supply are **user options**, every option implemented and tested, the recommended answer the default: `sizing.thin_material: resolve \| template` (default `resolve`), `sizing.thin_gap: template \| resolve` (default `template`), `sizing.far_field: capped \| uncapped` (default `capped`) — D-10, M-4.6 |
| R-F1 | One general framework, more robust and extensible, GPU-accelerated |
| R-T1 | VTU is the primary, fully specified output — subject to R4 |
| R-T2 | A reference-grade verification system, also a standalone subcommand able to validate external VTUs |
| R-T3 | A VTU renderer with filters, transparency, clipping and batch views, plus the snapshot workflow feeding it |
| R-P1 | Every stage runs multicore in CPU mode; a stage lands parallel or it does not land |
| R-P2 | Parallelism may not change a committed number: two runs are byte-identical, independent of thread count or completion order |
| R-N1 | **The external reference project is never named in this repository.** Cited by role. `grep -riI` for the name must return nothing outside third-party input data |

Out of scope for iteration 1: split-node displacement-discontinuity sheets (export contract
reserved), Tet10/high-order, periodic BC pairing, MPI, out-of-core meshing.

## 5. Difficulty-tier / model-assignment convention

| Tier | Meaning | Assignable models |
|---|---|---|
| **T1 — Routine** | Mechanical, fully specified: scripted edits, builds/tests, translation, plumbing | **GPT 5.6 Luna Max** or **DeepSeek V4 Pro Max** |
| **T2 — Standard** | Single-subsystem implementation against written spec; localized reasoning | **GLM 5.2 Max** or **GPT 5.6 Sol Medium** |
| **T3 — Expert** | Cross-cutting design, exact topology/corefinement, junction handling, numerical robustness, cross-stage invariants | **Kimi K3 Max** or **GPT 5.6 Sol Xhigh** |

Each subtask carries a **Multimodal** flag: `yes` = must inspect visual inputs (renders, ParaView
screenshots, cutaways); `no` = completable from text, docs, logs and source alone. Where `yes`, the
subtask names the visual inputs. **Since rev 3.4, R9 makes every subtask that changes the mesh
`yes`, whatever its row says: M-1.9's focus-region set, before and after.**

**Three standing instructions for every implementer, whatever the tier.** (1) Run the biggest case
(a8, then the reference cases) before believing a result from the small ones — every conformity
result from a1/a3/a6a in the record was true and none generalised (record §6.53). (2) Charge every
defect to the arm that emitted it before designing a fix (record §6.46, §6.66); `plc_path`,
`parent_cell`, `escalation_reason` exist for this. (3) State which measure a new operation decreases,
and how, before writing it (record §6.49).

---

## 6. The review — what the previous plan got right, got wrong, and left open

### 6.1 Right, and settled

- **P-3's premise held.** The fan was the defect and the kernel is exact where it runs: on a3
  §7.4-meshed carries 0.4 % of its interface area off the surface against the declined arm's 87 %
  (record §6.49); on a8 §7.4-meshed and the facet-split fan are at 0.0 % (record §6.53). The
  determinism objection that decided gate G6-0 is answered in code: the gated path is byte-identical
  at 1 and 8 threads (§2.6).
- **P-1's instrument was right.** `[V13]` read at the corners, with the signed offset beside the
  unsigned deviation, is what made every later decision measurable; it stays the P3 gate and
  becomes a FAIL gate (M-0.1).
- **P-4's sizing decisions stand**: `gap_cells ≥ 4` is a floor, not a knob (record §6.10);
  refinement is exhausted for the sub-cell body (record §6.16); the element budget is dominated by
  the background lattice, not the cut (record §2.2).
- **The refutation discipline.** Eighteen routes are closed with a number each (§6.4). They are the
  most valuable thing the record holds and the reason this plan can be short.

### 6.2 Wrong, or left open, in the previous plan

1. **The goal path is behind a flag, and the plan had no subtask to remove it.** Forty-four steps
   of P-3.13 landed behind `RUSTMSPT_PLC_PASS`; Part I never scheduled its retirement, and four older
   handles survive with it. R3 says a gate must not outlive its prototype. **M-1, then M-3.**
2. **Five of nine cases have never been through the gated path.** a2, a4, a6b, a7a and a7b were
   measured on the default path only; the a8 lesson (record §6.53: fidelity +38 %, `[V3]` broken with
   113,859 hanging nodes on a mechanism three smaller cases cannot exhibit) says that is not a
   result. §2.2 measures them; **M-1.1** owns whatever it finds.
3. **`[V13]` is WARN while the goal says P3 is absolute.** SPEC contracts §5 gates P3 at `1.0` and
   §4 makes `[V13]` WARN; `mesh-verify` exits zero on a mesh that fails P3. **M-0.1** makes it FAIL.
4. **The whole-cell centroid fan is still an arm.** R1 forbids it; the plan let it stand as "the
   declined arm" and fixed its declarations instead (record §6.66). It carries essentially all the
   remaining P3 damage, every `[V6]` two-component step, and every `[V9]` failure (§2.4). **M-2**
   makes the kernel complete enough to delete it.
5. **Quality was planned as a 3-D problem.** P-5 was written as IQD + smoothing; the measurement
   says 94.3 % of bad tets inherit a needle *face* frozen by J1 and no 3-D operation can reach them
   (record §6.54). **M-5** starts in the face.
6. **S9–S11 were two lines.** They are the difference between a debug snapshot and a deliverable:
   `mesh` exits non-zero today, the INP does not exist, `[V10]`/`[V11]` are skipped, and the domain
   trim R-D1 requires is unbuilt. **M-6.**
7. **P1 was never exercised.** A-3-ranked, A-4b, A-5 and A-9 (record §17.4) were never built; the
   real datasets under `workspace/data` (a yarn, four WAAM builds) have never been fed to the
   mesher; there is no robustness sweep. **M-4.4.**
8. **`[V2]` fails on a3 on both paths and on a6b and a8 on the gated path** (19/3, 1 and 6
   duplicate pairs one float32 ULP apart) and the record's
   last word was *"a conditioning problem … S5/G4-2, not S8"* (record §6.64) with no subtask.
   **M-4.1.**
9. **Two contract arrays changed meaning without a contract change.** `declare_labelled_boundary`
   made every one-component material boundary a tagged face on both paths, so `[V5]`'s "interface
   nodes" now include chamfer nodes and its distance metrics moved (a3 `over_10pct_share` 0 → 0.011)
   — correct, and undocumented. **M-0.1** records it.
10. **The record was 10,490 lines and `AGENTS.md` is 242 KB.** No external implementer can ingest
    them; the previous plan assumed they would. This document is written to be sufficient alone
    (Appendices A and B carry what it needs from the record, which is deleted), and **M-8.1**
    rewrites `AGENTS.md`'s meshgen section to cite rather than restate.
11. **Time was never a gate.** One change was refuted on cost (4 s → 10 min, record §6.31) and
    nothing since has been timed. **M-6.5** sets per-case budgets; every phase reports against them.
12. **The P3 gate is a corner test and cannot see a face through the body** (MG-02). Three corners
    on a piecewise-planar surface do not put the triangle they span on it; `good_cube.vtu`'s two
    interior interface triangles read `on_surface_area_frac = 1.0` against a cube that they cut
    through. Every P3 number in §2 is a necessary condition. **M-1.5** builds the containment
    criterion and re-baselines.
13. **The verifier can say PASS on a mesh that fails**, three ways (MG-01, MG-03, MG-08): the exit
    status counts printed findings, the final-stage box check relaxes to the mesh's own hull, and a
    self-declared final contract document is never validated for presence or references. **M-1.0**,
    before any gate in §1.2 is read again.
14. **Identity is per file, closure is borrowed, J1 conflicts are counted, orientation is per
    face** (MG-04, MG-05, MG-06, MG-07). None is reached by an acceptance case, which is how each
    survived. **M-4.0a, M-4.0b, M-2.0, M-4.8.**
15. **M-3 and M-4.1 depended on each other** (MG-12): Part II put M-4 after M-3 and M-3.1 waited
    for M-4.1. M-4.1 is now **M-1.6**, in the phase M-3 already waits for.
16. **Four arguments treated a measurement tolerance as a geometric licence** (MG-11): M-4.1's
    "quantise the input to `q`" (the q grid is not the lattice grid — `0.5` on a lattice plane
    rounds to `0.5000084…` off it — and S0 keeps raw coordinates behind quantised keys), M-5.3's
    "small enough that `[V13]` does not see it", M-4.5's vertex-only distance bound, and the
    "four-thousand-times" margin that compares a normalized length to a fraction of a local edge.
    Each subtask is re-scoped (M-1.6, M-5.3, M-4.5).
17. **S9–S11's order loses S9's and S10's results at the trim** (MG-13), the split export was a
    per-face node substitution that cannot be right at a node used by other same-side tets
    (MG-14), and the GPU certificate G1 omits the input conversion term (MG-09). **M-6.2, M-6.6,
    M-7**, each rewritten.
18. **§2.3's matched resolution was not matched** (found 2026-09-25 while specifying R-E4).
    `run_reference.py` took the reference log's `Base Mesh Element size` — the background cell's
    face diagonal, `√2 ×` its edge — for the edge, and the first `SAMR levels:` line where the
    reference sets one level per section; the octree's longest-axis power-of-two ladder then moved
    both again. Cases 1 and 2 ran at 1.0625× and 0.9375× the reference's cell edge, case 3 at
    1.41× its background edge and 2.81× its finest. A measured row with a runnable source can
    still measure the wrong thing when the source's *parameters* were never checked against the
    thing compared — the fifth such row in the record (record §2.0 was the fourth, and it carried
    this flaw forward). **M-1.8.**

### 6.3 Gaps in the specification freezes

Found by reading the code against the three `SPEC_*.md` files. None changes a frozen *table*;
every one is behaviour the code has and the spec does not describe, or a spec clause the code
contradicts. **M-0** closes them in one revision each, with §14/§15 records.

| # | Spec | Gap | Owner |
|---|---|---|---|
| S-1 | geometry §7.6 | Specifies *curve-pinned sequential kirigami* as the fallback; built is a centroid fan → facet-split fan (record §6.2 called this "an undeclared deviation" and it is still undeclared in §15) | M-0.1 records; M-2 deletes the whole-cell fan; the facet-split fan is then the only fallback and is specified |
| S-2 | geometry §7.4 | "Steiner points are permitted **off** the constraints only"; the boundary recovery the refusals need (§2.4) inserts Steiner points **on** a facet's interior, strictly inside the cell. Compatible with J1 (no shared face touched) and with the exact-average rule (a facet-vertex average lies in the facet) | M-0.2 amends after M-2.1 measures |
| S-3 | geometry §7 | Unspecified behaviour: cap clipping by cut history, identical-cap merge, `conform_cap_rim`, the T-junction detector/repair, `curve_mesh_edges`' carriage rule (both ends and midpoint within `eps`), `[R1]` cited in `cdt.rs` but defined only in the plan | M-0.1 |
| S-4 | geometry §15 | Open item "junction CDT viability → gate G6-0" is answered: the kernel exists (`cdt.rs`, hand-rolled, 5,940 lines) and is exact where it runs; numerics §11's "no adopted crate" likewise | M-0.1 closes both |
| S-5 | geometry §15 | `cut.rs:14–23`'s two deliberately-unimplemented remedies (K1 refine-in-cut, §4.4 steps 1–2) are not in the deviations table | M-0.1 |
| S-6 | geometry §7.3 | `cdt.rs:47` says the `FaceTriCache` "is not implemented"; it landed 2026-08-15 and was made authoritative — stale doc, and the spec's fingerprint-mismatch-is-hard-error clause should be checked against what shipped | M-0.1 |
| S-7 | contracts §4 | `[V13]` WARN vs §5's absolute P3 gate; `[V6]`'s `undeclared_boundary_faces`, `region_adjacency_violations` and the mixed-priority exemption are unspecified; `duplicate_node_tol_frac = 1e-6`, `hanging_tol_frac = 1e-9`, `plane_tol_frac = 1e-9` are code-only | M-0.1 |
| S-8 | contracts §4 `[V9]` | "A curve the mesh carries no edge for is reported INFO, not failed: gate G6-0 adopted the conforming fan" — void under R1. Curve carriage becomes a FAIL clause once M-2 lands | M-0.2 |
| S-9 | contracts §2 | `plc_path`, `parent_cell`, `escalation_reason`, `node_origin` are diagnostic arrays with no contract status (dump-only under `RUSTMSPT_CUT_DIAG`) — list them as non-contract | M-0.1 |
| S-10 | geometry §15 | Open item "post-snap transition quality → gate G4-3": measured on fixtures (`tests/meshgen_quality_gate_tests.rs`, a non-empty transition population, pre-snap / post-snap / post-cut) but never on the acceptance and reference matrix — the review's erratum (b) corrected v3's "never measured" | M-5.1 measures it on the matrix; geometry rev 1.6 records the fixture result |
| S-11 | numerics §1.2 | The weld grid `q` applies to mesh nodes; nothing quantises the **input**, which is why two trace endpoints can differ by one float32 ULP (record §6.60) | M-4.1 decides and M-0.2 freezes |
| S-12 | contracts §5 | The feature-curve conformance (≤ 10 % `h`) and sharp-corner error (≤ 1 % `h`) rows are gated by `[V5]` on paper and **not implemented** — `verify.rs` has no such metric; G2's preservation half has no number | M-0.1 records; M-6.3's verifier work implements |
| S-13 | record §10.2 / geometry §1 | "Optional explicit point-list overrides accepted" for sharp features — a module-doc sentence in `features.rs`, no config field, no code. Either implement (G2) or strike from the record and the config sketch | M-0.1 decides; if kept, M-4.6 |
| S-14 | contracts §4 / §5 | `[V13]`'s corner criterion is necessary, not sufficient, for P3 (MG-02): `good_cube.vtu` against a `[0,1]³` cube STL reads `on_surface_area_frac = 1.0` with two interface triangles through the cube | rev 1.3 records; **M-1.5** implements containment |
| S-15 | contracts §4 | The item cap changes the failure count and exit status (MG-01) | rev 1.3 §4.3 normative + D-9; **M-1.0** |
| S-16 | contracts §4 `[V3]` | The octree-hull relaxation applies at stage 11 (MG-03) | rev 1.3 §4.4 normative + D-10; **M-1.0**, **M-6.2** |
| S-17 | contracts §0 / §2 | A self-declared final contract document is not validated for **A**-presence or semantic references (MG-08) | rev 1.3 §4.2 normative + D-11; **M-1.0** |
| S-18 | geometry §7.3 | A J1 fingerprint mismatch is counted, not raised; the fingerprint is the component-id set, not the constraint entity ids (MG-06) | rev 1.6 D-18; **M-2.0** |
| S-19 | contracts §2.3 | `FaceTagOrientation` is written once per face, not per member, and all `+1`; `FaceTagSideElems` takes the first tag's pair (MG-07) | rev 1.3 D-13; **M-4.8** |
| S-20 | numerics §6.1 | Certificate G1 bounds arithmetic on f32 operands as given; the f64→f32 conversion of the inputs can flip a near-coplanar sign (MG-09) | rev 1.3 D-13 records; rev 1.4 repairs before GK-1 (**M-7**) |
| S-21 | numerics §1.2 | The four roles of a coordinate — identity key, ordering key, geometric coordinate, error metric — were unstated, and two subtasks argued from one role's tolerance to another's move (MG-11) | rev 1.3 §1.5 + Rule N14; **M-1.6**, **M-5.3**, **M-4.5** |
| S-22 | geometry §9.1 / R-A2 | One X per input file, not per connected closed component (MG-04) | rev 1.6 D-19; **M-4.0a** |
| S-23 | geometry §12 ARB-4 | Closure is proved from a global edge incidence, so another component's face closes this one's opening (MG-05) | rev 1.6 D-20; **M-4.0b** |
| S-24 | contracts §2.1 | `partition_id` is `0` on every cell, non-tets included; no flood fill has ever been produced (MG-13) | rev 1.3 D-14; **M-6.1** |
| S-25 | contracts §2.4 / §3 | Index 10 is `quality`; v3's `s10_regions` gave it two meanings (MG errata a) | rev 1.3 D-15; M-6.1 corrected |
| S-26 | geometry §8.1 / §11 vs P3 | Sheet collapse puts the sheet on neither original wall and changes connectivity; P3 against which surface is undecided (MG-10) | §1.3; **D-6**, **D-8** |
| S-27 | geometry §4.4 / contracts §4 `[V4]` | A uniform quality floor cannot be met inside an input-forced wedge below it (MG-10) | §1.3; **D-7** |
| S-28 | geometry §1.2 | S8 orders on `1e-6·ε`, not `1e-6·q` as Rule K-O, §14 [10] and D-16 stated; S7 keys on `ε`; `[V2]` on `1e-6·diag`; S0 keeps raw coordinates behind quantised keys (the audit) | rev 1.6 D-35 records; the constant's doc comment is fixed at **M-3.3** |
| S-29 | geometry §4.4 / §6 | The 8° floor is not applied to §6 pieces (`Quality` / `[CUT-PRISM]` unreachable); band slabs take an unchecked fan; the flip, checked Steiner rung and regional demotion live in `mesh_band_layer`, which nothing calls; `[V7]`'s `ThinSkipRegion` has no producer (the audit) | rev 1.6 D-29; **M-4.3** decides the band ladder's caller; `[V7]` joins R8's list |
| S-30 | geometry §5.2 / §6 | `split_R` has no producer; `split_4` is unreachable from §6; two added rows (I, i) and the table-vs-interior check are undocumented (the audit) | rev 1.6 D-30, D-31 |
| S-31 | geometry §8 | Band cells are lattice cells found by the doubly-cut face rule, not Φ-spanned prisms; collapse is per lattice edge behind `collapse_sheets`; `k = 1, 2` unreachable; band tets seeded per tet (the audit) | rev 1.6 D-32, new §8.3/§8.4; **M-4.3** |
| S-32 | geometry §9.2 row 11 / §12 ARB-23 | An `ambiguous` entry is dropped, not an error; no sweep, no `[OWN-*]` tag, `arbitrated` never written; T-R1 pins the drop (the audit) | rev 1.6 D-33; **M-6.1** decides the sweep |
| S-33 | geometry §11 | `h⁽⁰⁾ = h_max`; `C(R)` was never defined; `τ_l ≤ 1` only post-loop; S3-M falls back to `t_raw`; the oscillation and cap-lock definitions differ from the text (the audit) | rev 1.6 D-34; two open items recorded there |
| S-34 | geometry §12 / §13 | `[THIN-LOCK]`, `[THIN-LADDER]`, `[CUT-PRISM]`, `[OWN-*]` tags do not exist; T-J1 is met at the kernel only; T-R1 covers 9 + 1 + the opposite of row 11 (the audit) | rev 1.6 D-39, §13 status column |
| S-36 | contracts §4.1 / numerics §5 Rule N7 | The verifier's report is not deterministic on a large mesh: `[V6]` items in `HashMap` order under the cap; one float metric summed in that order (measured twice on A-3) | rev 1.3 D-17 / numerics D-17; **M-1.0 (e)** |
| S-37 | numerics §2 S1 row | The S1 corner test compares `cos²` to a literal `cos²(60°)` with no sign guard, so a hairpin sharper than 120° is not a corner (the audit) | numerics D-21; **M-4.6** fixes the code |
| S-38 | geometry §12 ARB-4 | An open declared solid with `\|w\| ≤ 0.5` is reclassified `Sheet` silently, with no WARN (the audit) | rev 1.6 §12 as-built; **M-4.0b** |
| S-39 | numerics §0 / §1.3 | Twenty-odd named constants and literals outside §1.2's ladder, two of them absolute (the fragment kernel's arena quantum is a volume fraction reused as a length); `tet_signed_volume` is `robust`'s magnitude used as a number; Rules N4/N6 hold in S2 only (the audit) | numerics §1.3, D-15, D-18, D-19; **M-3.3** |
| S-40 | numerics §1.2 / §4.3 | `κ_esc` is `c = 25` for C2, not only `c = 7`; C1/C3 compute DD on every construction (the audit) | numerics D-20 |
| S-41 | geometry §12 ARB-11 / contracts §4 `[V5]` | S7's cap drops rather than clamps and `under_snapped` is read by nothing; ARB-11's `[V5]` gate on under-snapped nodes does not exist (the audit) | numerics D-22; the `[V5]` clause is contracts rev 1.4's if wanted |
| S-42 | geometry §10 C1–C3, C5–C6 | A coincident patch bordered by any single-owner face is C3, not C1/C2; C5/C6 are split by coplanarity and fire on a body's own vertex adjacency; rows apply to self-coincidence (the audit, probed) | rev 1.6 D-40; **M-4.0** decides code or table |
| S-43 | geometry §10 C10 | No box-clip curve for a sheet; the box tag is gated on the primary tag, so a C9 ∩ C10 sheet is never box-tagged; `FaceTagKind` is single-valued (the audit, probed) | rev 1.6 D-41; **M-4.0** |
| S-44 | geometry §10 Invariant M1 / §12 ARB-4 | A component whose faces were all merged under another's primary tag is unclassified at S2b and defaults to `Sheet` at S6: two identical solids at equal priority deliver one and lose the other's material (measured end to end) | rev 1.6 D-42; **M-4.0b** widened; fixture A-16 |
| S-45 | numerics §6.3 / §11 D-7 | No GPU GWN exists; the CPU GWN is a sequential `atan2` sum with `\|w\| > 0.5` and no band; `gwn_margin_band` is dead code at `2⁻²³` (the audit) | numerics D-25; **M-7**, M-3.3 |
| S-46 | numerics §7.1 Rule N10 / §9 T-N1..T-N11 | The sign convention is applied at three sites plus a raw `insphere` call; the T-N1/T-N7 lints cover S0–S2 modules only; T-N2 lacks the error-bound assertion and any f32 case; T-N4 and T-N10 do not exist; T-N11 is one-thread-vs-default (the audit) | numerics D-24, D-26; **M-1.0** |
| S-47 | numerics §6.1 / §2 S6 | The plane-test perturbation's `4·EPSILON` filter bounds the dot product, not the rounded normal it is taken on (the audit, code reading) | numerics D-23; M-3.3 |
| S-48 | contracts §1 | The `mesh` pipeline writes ascii always; the delivered file carries the face/curve tables and gives `[V7]`/`[V9]` false answers; `s04` is written whole; `--split` has no producer (the audit, measured on A-3/A-6a) | contracts D-18, D-19; **M-1.0** (skip with a named reason), **M-6.2** |
| S-49 | contracts §2 | `provenance` `0` on non-tets; `ThinRegionSkip` code 6; `ThinRegionSeparation` normalized; `verify_flags` counts WARN; `constraint_ref` `−1` for corner/box; `snap_motion`/`GlobalPointId` rows missing; `node_origin` always written (S-9 corrected) (the audit) | contracts D-16 corrected, D-20 |
| S-50 | contracts §2.3 `CurveRadialPatches` | Every value is `0` on pipeline output because the box clip drops S2's radial order, so `[V9]`'s radial clause has never run on a mesh (the audit) | contracts D-21; **M-4.0** carries the order through the clip |
| S-51 | reference docs | `docs/en-us/reference/meshgen.md`'s G6-6 post-cut row is stale against the test at `891badc` (the review's erratum, re-measured) | **M-8.1** |
| S-52 | geometry §8.1, contracts §2.2/§2.3 | G7-2's collapse is per vertex pair and discards the two wall crossings it averaged; nothing in the contract records a sheet's thickness or the wall points, so no export can put a sub-`t_sheet` gap back on its walls (§12.1) | **M-6.7** specifies `SheetPairOffset` and `SheetThickness`; contracts rev 1.4 (**M-0.2**) freezes them |
| S-35 | numerics §0 / §2 | Named constants outside §1.2's ladder (twenty-odd, several decision-bearing, two absolute); two float side decisions in S8 (the audit) | rev 1.3 §1.3, D-15, D-16; **M-3.3** names or retires each |
| S-53 | geometry §2.3 | The stated reason the primary lattice is Freudenthal rather than the 5-tet scheme — the XOR-parity checkerboard's "bookkeeping does not survive contact with octree level transitions" — does not hold under §3's own machinery: a parity rule stated per face, at the face's own level, is a pure function of the face (Invariant C), so T1's proof carries over verbatim. Appendix D.1 finds zero unpaired faces and zero hanging nodes on seven strongly balanced configurations (edge-only, vertex-only and three-level included) for the uniform table, the parity-reflected table and the 5-tet checkerboard alike, and all three fail the unbalanced negative control (rev 3.3) | **M-1.7** — geometry rev 1.7 corrects the rationale together with the rule it changes |
| S-54 | geometry §3.1 as-built (D-38) / `sizing.rs` | The octree root is one cube on the domain's longest axis (`sizing.rs:1417–1418`) and the background is that root halved until it is `≤ h_max` (`forest_level`, 1425–1430), so the only expressible backgrounds are power-of-two counts along the longest axis; `SIZING_MAX_LEVEL = 12` counts from that root, not from the background; `L = 0` (a uniform lattice) is rejected by `h_min < h_max` (rev 3.3) | **M-1.8** |

**Status 2026-09-23.** Every S-row above marked "M-0.1 records" is recorded in the revised freezes
(geometry rev 1.6, numerics rev 1.3, contracts rev 1.3); the rows that need code (S-2, S-8, S-11,
S-12, S-13's decision) keep their later owners. The revisions record each as-built behaviour as a
deviation with its owning subtask, never as an accepted design. A check-by-check account of what
the audit found in each freeze is in that freeze's verification record.

### 6.4 The do-not-retry register

Every route below was built, measured and reverted, with the number. An implementer who proposes one
of them must first say what is different now.

| # | route | measured, and why it is closed | record |
|---|---|---|---|
| X-1 | Snap lattice nodes onto feature curves in S7 | 24 of 1,404 segments covered; edge-aware doubling to 53 made P3 worse; geometry bound 0.707 `l_min` exceeds any valid cap | §6.4 |
| X-2 | Refine toward the sub-cell body | field already at `h_min` on every case; a8 92.011 → 91.980 % for +1.8 % elements; h/2 reaches 88.5 % for 2.76× | §6.16, §6.11 |
| X-3 | The spoke cut (plane through three crossings) | proxy −26 %, serration visually intact, a7a's plate 2.6× thinner-than-true; shipped behind a flag — reverted, and the flag was the finding | A-§9837 |
| X-4 | Widen the crease/junction fan's condition | worse on P3 while holding `[V3]`, twice | §6.6, §6.18 |
| X-5 | Conforming face triangulation by splitting segments until Delaunay | a6a 4 s → over 10 min; elements up for the same gain | §6.31 |
| X-6 | Narrow §7.1's triage to node-set inadequacy | 13× fewer cells offered, P3 **worse** on both cases | §6.45 |
| X-7 | Steiner cone in the link for `remove_edge` refusals | built 687 of 692, converted 2 cells: a cone adds `n+2` edges through the very region the facet crosses | §6.49 |
| X-8 | Interior Steiner points as the first quality move | 94.3 % of bad tets inherit a 2-D needle face; the remedy reaches 6 % | §6.54 |
| X-9 | Edge split with deletion for the last hanging nodes | a8 1 → 6 hanging incidences; validated on a3 only | §6.61 |
| X-10 | Merge / collapse the one-ULP coincident pairs | unconditional: 103 leaks, 513 non-manifold; link-validated: 15 non-manifold, 185 hanging — they share no tet, so they are not edges | §6.64 |
| X-11 | Widen the trace-endpoint snap to `eps` | a3's traced cells 16,883 → 190 — a shared node must lie on the two faces' common line | §6.65 |
| X-12 | Remove the `on_cell_face` cap filter | a3 refusals 334 → 242 and `[V3]` breaks (3 multi-shared / 9 non-manifold; a6a 2 / 6) — R1 | §6.66 |
| X-13 | Place a boundary triangle by a sample toward the cell centre, or by the run its cap's rim does not separate (three forms) | neutral to worse: a6a builds 304 → 279; or +731 tets, +2 duplicates, `[V9]` −6, `[V6]` unchanged | §6.66 |
| X-14 | Declare a two-component step from the labels alone | makes `[V6]`'s coincidence exemption vacuous; a3's `{1}\|{2}` faces at `x = 0.5` are chamfers, not contact | §6.66 |
| X-15 | `gap_cells < 4` | no lattice vertex lands in the gap; a floor, not a taste | §6.10 |
| X-16 | Force a curve-pierced face to escalate | identical `[V6]` at +2 % tets | `AGENTS.md` G6-0 lesson |
| X-17 | The `meets_inside` gate; `RUSTMSPT_NO_SPOKE_CUT`; `output.split_volume` | each a setting between a correct and an incorrect mesh — deleted, not defaulted | A-§6.2 (P-3.2), A-§9837, A-§10.14 (P-2.1) |
| X-18 | Relabel the two-component-step tets | every claimed component has the tet's centroid **and** a node inside it; the labels are right and the intermediate region is missing | §6.66 |

### 6.5 The 2026-09-11 review (MG-01..MG-14), merged

Written against `891badc` (package `0.2.1`, CPU release, `aarch64`) after reading this plan, the
three freezes and the reference docs, and checking S0/S2b, the S7/S8 interface, the face cache, the
cut output, the predicates, snapshots, the verifier and the acceptance runner — not a line-by-line
audit of the 42,641-line module, and not a re-run of the nine-plus-three matrix. Its evidence
classes: **reproduced** = a counterexample was executed (Appendix C); **static** = the full call
chain was read and the behaviour is unambiguous; **design gap** = an unbuilt part of this plan that,
built as written, would not meet its goal. Priority **P1** = makes a result wrong, an acceptance
false, or a goal unreachable; **P2** = must be closed before the feature it concerns is delivered.
Every row was re-verified on 2026-09-23 at the same commit — the reproduced rows by re-running the
script, the static rows by reading the cited code again (§13).

| id | pri | finding | class | re-verified 2026-09-23 | owner |
|---|---|---|---|---|---|
| MG-01 | P1 | The item cap changes the failure count and the exit status: `VerifySection::push` sets the status before the cap, `verify_with_options` counts only stored items, `passed()` reads those counts | reproduced | reproduces (`hidden_failure`: `V1 = FAIL`, `fail = 0`, exit 0) | M-1.0 |
| MG-02 | P1 | `[V13]`'s corner criterion is necessary, not sufficient, for P3; two interior triangles through a cube pass at `on_surface_area_frac = 1.0`; `check_v13` can also return SKIPPED when the boundary is missing entirely | reproduced | reproduces (`corner_only`: `[V13]` PASS, `chord_max = 0.5`) | M-1.5 |
| MG-03 | P1 | `check_v3` builds a hull from the mesh's own points when they exceed the domain and accepts free faces on it at every `StageIndex` including 11; `check_delivered` reads the same codes | reproduced | reproduces (`final_outside_domain`: PASS, `boundary_leaks = 0`) | M-1.0, M-6.2 |
| MG-04 | P1 | One input file is one semantic component: `ArrangeComponent` per `p.inputs` entry, `kind` from the whole file's closedness; S0's connected components never become the semantic table; the verifier maps `X − 1` to the `surfaces` index | reproduced | reproduces (`two_solids_one_file`: `ComponentX = 1`; `solid_and_sheet_one_file`: `ComponentKind = 1`, `ComponentClosed = 0`) | M-4.0a |
| MG-05 | P1 | `rebuild_topology` proves a component's closure from a global `edge_faces` incidence, so another component's face closes this one's opening; the member set uses the representative `face.component`, not every tag; incidence > 2 is not treated as a defect when no edge has incidence 1 | reproduced | reproduces (`borrowed_closure`: `ComponentClosed` `[0,0]` → `[1,0]`) | M-4.0b |
| MG-06 | P1 | `check_face_cache` turns `FingerprintMismatch` and a triangle-set disagreement into a counter and `None`; the caller keeps its own triangulation; one warning line at the end. The fingerprint is the component-id set plus a crease flag, not §7.3's constraint entity ids | static | holds (`cut.rs` `check_face_cache`; the comment says "counted rather than raised") | M-2.0 |
| MG-07 | P1 | `cut_to_doc` pushes `FaceTagComponents` per tag and `FaceTagOrientation` once per face, all `+1`; `FaceTagSideElems` takes the first tag's pair | reproduced | reproduces (`contact_orientation`: 280 members, 270 orientations) | M-4.8, M-1.0 (validator) |
| MG-08 | P2 | A self-declared `SchemaVersion = 1, StageIndex = 11` document is not validated for **A**-presence or semantic references: missing `constraint_kind`/`constraint_ref`, out-of-range `FaceTagSideElems`, and a `sheet` component owning tets all pass | reproduced | reproduces (three cases, each `fail = warn = 0`, exit 0) | M-1.0 |
| MG-09 | P1 | Certificate G1 bounds the arithmetic on f32 *inputs*; the f64→f32 conversion of the inputs can flip a near-coplanar sign, and G2 inflates only the broad-phase boxes. Counterexample: f32 `det = −3.576e-9` accepted against bound `1.554e-15`; exact f64 `det = +1.600e-10` | numeric counterexample | reproduces (`gpu_g1`; python `Fraction` vs step-wise f32; no GPU meshgen kernel exists to test) | numerics rev 1.3 (recorded), M-7 |
| MG-10 | P1 | "Any input + exact surfaces + one uniform quality floor" is not always satisfiable (the 1° wedge); sheet collapse to the midpoint violates P3 against both walls and changes connectivity; P1's "no hard failure" conflicts with R1 and `repair: strict` | mathematical counterexample, design gap | holds by argument | §1.3; D-6, D-7, D-8 |
| MG-11 | P1 | Quantisation and movement arguments treat a measurement tolerance as a geometric licence: quantised keys are not quantised coordinates (S0 keeps the first vertex's raw coordinates); the q grid is not the lattice grid (`0.5/√3` rounded to `1e-5` returns `0.5000084271289835`); `(q/2)/(0.02·h_local)` has no fixed margin and the 3-D rounding bound is `√3·q/2`; M-5.3's move is invisible to `[V13]`, not on the surface; M-4.5's vertex bound does not bound the new triangles' interiors, self-intersection or topology | static, design gap | holds (`surface.rs` weld keeps first coordinates; the arithmetic re-run) | M-1.6, M-5.3, M-4.5; numerics rev 1.3 §1.5 |
| MG-12 | P2 | Part II's order (M-4 after M-3) and M-3.1's dependency on M-4.1 form the cycle M-3 → M-4.1 → M-3 | text | held; resolved by moving M-4.1 to M-1.6 | Part II |
| MG-13 | P1 | S11's trim is scheduled after S9 quality and S10 labels; a box plane through a tet creates new nodes and slivers and invalidates element ids, side pairs, `N_ID` and partitions; M-6.1's "partitions exist in `cut_to_doc`" is false — it writes `vec![0; cells]` with no sentinel | design gap, static | holds (`cut_to_doc` `partition_id`; the pipeline stops after S8) | M-6.2, M-6.1 |
| MG-14 | P2 | The split export as a per-face `elem⁻` remap leaves same-side tets not adjacent to the face on the old node; "two copies per node" fails at three-interface junctions, multi-material junctions and crack fronts; side pairs mix a geometric side with a per-component inside/outside | design gap | holds by argument | M-6.6 |

**The review's four errata, applied.** (a) *Snapshot index 10.* M-6.1's `s10_regions` collided with
the frozen enumeration (`9` thin, `10` quality, `11` final; contracts §2.4/§3, `snapshot.rs`): S10
emits no snapshot of its own and its tables are part of `s11_final` (M-6.1). (b) *"G4-3 was never
measured" was too strong.* `tests/meshgen_quality_gate_tests.rs` measures pre-snap, post-snap and
post-cut quality on a non-empty transition population, and the English reference records the
result; the correct gap is "not measured on the acceptance and reference matrix", which is what
M-5.1 now says; S-10 in §6.3 is reworded. (c) *R3's grep was too wide* — narrowed in §3 (R3). (d)
*The English reference carries stale defaults and status claims* (`gap_cells`, `[V9]`, S8
exactness) — listed under M-8.1, not fixed here, and no reference statement is copied into a spec
as normative without a check against the code.

**The review's implementation order** is adopted as Part II's order: trustworthy acceptance first
(M-1.0, M-1.5, and the D-6..D-8 definitions), then identity and conformity (M-2.0, M-4.0, M-4.8),
then the acyclic plan (M-1.6 before M-2 and M-3, re-baselined under the trusted verifier), then
final geometry and delivery (M-6.2's trim order, M-6.6's split semantics, S9–S11 and the non-cubic /
sharp-angle / open-sheet exported-mesh acceptance), and GPU last, after the certificate is fixed.

---


# PART II — the work

Ordered so that every phase is measurable by the one before it, and so that the dependency graph
is acyclic (MG-12 found v3's was not: M-3.1 waited for M-4.1 while M-4 waited for M-3). M-0 (spec
text) is done for what is built and reopens after M-2 and M-5. **M-1 lands first and now holds
everything a later gate reads**: the verifier's own defects (M-1.0), the P3 criterion (M-1.5), the
harness, byte-identity, budgets and the P2 baseline, and the identity rule for the one-ULP pairs
(M-1.6, moved from M-4) — and, rev 3.3, the two owner requirements that change what every later
baseline is measured on: the resolution ladder (M-1.8) and the X-pattern lattice (M-1.7). M-1.7
changes which tets the lattice hands S8 and nothing about how S8 cuts them; M-2 is the only phase
that changes what a cut cell becomes, and starts with J1 made a hard error (M-2.0). M-3 deletes the flag once M-2 has made the gated path not-worse on every
FAIL gate under the trusted verifier. M-4 and M-5 are independent of each other and run in parallel
after M-3; M-4 opens with the component-identity fixes (M-4.0) the P1 campaign's new fixtures need.
M-6 turns the snapshot into a deliverable, with the trim moved ahead of quality (M-6.2). M-7 waits
for the certificate repair; M-8 closes. Dependencies, explicitly: M-1 → M-2 → M-3 → {M-4, M-5} →
M-6 → M-7 → M-8; M-0.2 after M-2 and M-5.2; D-6..D-8 before M-1.5's gate is *read*, not before it
is built. Every phase reports against §2.2's matrix, re-baselined by M-1.0 and M-1.5, and names the
rows that moved.

## 7. Phases and subtasks

### M-0 — Freeze what is built (spec revisions)

Two steps, because a freeze must not describe something unmeasured (R6).

- **M-0.1. As-built revision — text half done 2026-09-23 (docs only; the code half is M-1.0).**
  `SPEC_meshgen_geometry.md` rev 1.6, `SPEC_meshgen_numerics.md` rev 1.3 and
  `SPEC_meshgen_contracts.md` rev 1.3, closing §6.3's S-1, S-3, S-4, S-5, S-6, S-7, S-9 and
  recording S-14..S-27 (the review's), each as a deviation with its owner. In the geometry spec: a new §7.7
  "As built (2026-08-27)" describing the facet-split fan (per-piece cap clipping by cut history,
  identical-cap merge, `conform_cap_rim`), the T-junction detector and repair with its fixed-point
  guard, the curve-carriage rule, and `[R1]`'s text moved from the plan into §0; §15 gains rows for
  the centroid fan (as a deviation to be **removed** by M-2, not accepted), for `cut.rs:14–23`'s two
  unimplemented remedies, and for the FaceTriCache's status; §15's G6-0 open item is closed with
  the P-3.13 record cited. In the contracts spec: `[V13]` becomes **FAIL** at `on_surface_area_frac
  = 1.0` with `interface_on_surface_frac` as its anchoring tolerance (the same clause §5 already
  states); `[V6]` gains `undeclared_boundary_faces` (FAIL when > 0) and the mixed-priority
  exemption as written in `verify.rs:1447–1472`; the three code-only tolerances are frozen; `[V5]`'s
  interface-node set is redefined as "every tagged face", with the note that since
  `declare_labelled_boundary` that is every material boundary; the four diagnostic arrays are listed
  as non-contract.
  *Acceptance (text half, met 2026-09-23):* every new table row has a verification-record entry
  naming its source; no dangling cross-reference (the specs' "plan §N" citations resolve through
  B.0); every as-built deviation carries its owning subtask. *Acceptance (code half, M-1.0):*
  `mesh-verify` on the nine contract documents at `891badc` reports exactly the statuses §2.2
  records with `[V13]` moved from WARN to FAIL on eight cases, and every other movement charged to
  the verifier defect that hid it. *Tier T3 (geometry) / T2 (contracts). Multimodal: no.*
- **M-0.2. Post-M-2/M-5 revision.** Geometry rev 1.8 (rev 1.7 is M-1.7's and M-1.8's, §8): §7.4's
  Steiner rule amended as M-2.1 measures it (S-2); §7.6 rewritten so the facet-split fan is the
  fallback and the whole-cell fan does not exist; the input-quantisation rule from M-4.1 (S-11) in
  numerics §1.2. Contracts rev 1.4: `[V9]`'s curve-carriage clause becomes FAIL (S-8). *Tier T3.
  Multimodal: no.*

### M-1 — Measure and hold

Nothing below can be judged without a harness that runs both S8 paths on all twelve cases, keeps
every report, and charges every defect to an arm. Today the runner discards the mesher's stdout,
overwrites the previous path's reports, and does not time anything.

**Order inside M-1 (rev 3.3; M-1.9 added rev 3.4).** M-1.0, M-1.5 and M-1.1 first — the trusted
verifier, the P3 criterion, and a harness that reproduces §2.2–§2.4 at `0a8eb1c` — with M-1.9's
focus renders landing beside M-1.1, so that the first mesh-changing subtask has a "before". Then M-1.8 and M-1.7, which
change the resolution the reference cases run at and the lattice every case runs on, M-1.8 first
so that each change is charged to its own row. Then M-1.2, M-1.3, M-1.4 and M-1.6, whose
baselines would otherwise be taken at resolutions and on a lattice about to be replaced — M-1.6
in particular, because which lattice diagonal planes an input plane can nearly coincide with is
exactly what M-1.7 changes.

- **M-1.0. Verifier trust.** Three defects of `mesh-verify` at `891badc`, each with its negative
  fixture first (R8), plus the code half of M-0.1. (a) **MG-01** — `VerifySection::push` sets the
  section status before the item cap and `verify_with_options` counts `fail`/`warn` over the stored
  items only, so `max_items_per_section: 0` on `bad_inverted_tet.vtu` reports `V1 = FAIL`,
  `summary.fail = 0`, exit 0, and a section whose INFO items fill the cap hides a later FAIL.
  Severity counts accumulate independently of storage; `passed()` reads section statuses; every
  `.take(cap)`-before-`push` path (`hanging_nodes` and its siblings) is audited, not only the
  summary. Fixtures: cap 0/1/50 give the same exit and counts on one defect; an INFO-then-FAIL
  section. (b) **MG-03** — `check_v3` builds a hull from the mesh's own points whenever they exceed
  `DomainMin/Max` and accepts free faces on it at every `StageIndex`, 11 included: `good_cube.vtu`
  with every `x` doubled and `DomainMax = [1,1,1]`, `StageIndex = 11`, passes with
  `boundary_leaks = 0`. The hull relaxation is admitted only for the pre-trim stages, with a
  stage-named defer; at 11 every used node lies inside the box, every free face lies on a box
  plane, and the tet volume equals the box volume within `[V6]`'s tolerance. Fixtures: the doubled
  cube, a translated mesh, an extra block outside the box, an interior cavity, and the pre-trim
  overhang of a non-cubic domain (allowed, reported). (c) **MG-08** — a document declaring
  `SchemaVersion = 1, StageIndex = 11` is not validated: removing `constraint_kind`/`constraint_ref`,
  setting `FaceTagSideElems` to `[999999, 999998, …]`, or setting `ComponentKind[0] = sheet` while
  the component owns three tets each passes with `fail = warn = 0`. A stage-sensitive contract
  validator, selected from the metadata and never from array absence, distinguishes the primary
  volume, the mixed contract and an external geometry-only VTU and checks **A**-presence, types and
  component counts, sentinels, set offsets, component existence, sheet-never-volume, per-member
  orientation length (MG-07) and side-element range, cell type, shared face and sign; an external
  VTU may still be verified geometry-only, explicitly. Fixtures: the three injections and the MG-07
  document. (d) `[V13]` becomes **FAIL** at `on_surface_area_frac = 1.0` and `[V6]`'s
  `undeclared_boundary_faces` FAIL when `> 0`, as contracts rev 1.3 states them.
  (e) **Verifier determinism (numerics D-17):** `[V6]`'s region-adjacency items are pushed in
  `HashMap` iteration order and truncated at the cap, and `undeclared_boundary_area` is a float sum
  in that order — two identical `mesh-verify` runs on A-3's contract file list different items and
  differ in the last digits of that metric. Sort the keys as `check_v3` does and sum in index
  order; the fixture that pins it must exceed the cap.
  *Acceptance:* every new fixture in contracts §6's manifest with its exact fired set; the review's
  `reproduce.py` cases `hidden_failure`, `final_outside_domain`, `missing_required`,
  `invalid_side_elements`, `sheet_claims_volume` and `contact_orientation` each report a named FAIL
  and non-zero exit; two runs on A-3's contract file give one JSON; `check_delivered` in `run_acceptance.py` reads a `[V3]` that no longer relaxes
  at stage 11; the §2.2 statuses re-measured under the trusted verifier, with every row that moved
  named — the spec change moves `[V13]` to FAIL on eight cases, and anything else that moves is a
  defect the old verifier hid. *Tier T2. Multimodal: no.*
  **Status 2026-09-26 — landed (committed with M-1.1).** (a)–(e) built, plus contracts D-18's verifier
  half (on the delivered tets-only volume `[V7]`/`[V9]` skip and `[V6]`'s declaration rules
  report `V6.deferred`) and the `partition_id` sentinel (D-14's `−1` on non-tets). Contracts
  rev 1.3.1 records it (D-8..D-11, D-17 closed; D-22..D-25 new). 656 tests green; new tests pin
  the cap (0/1/50), INFO-then-FAIL, push-order independence, the doubled and translated cube, the
  pre-trim overhang and `delivered`, the three MG-08 injections and MG-07, and a fold. The
  fixture generator now restates `FaceTagSideElems` after mutating (seven fixtures rewritten,
  `good_cube.vtu` byte-identical). **Re-measured on both paths at the current tree**
  (`data/output/m10/{default,gated}.json`; tets and on % reproduce §2.2 to the digit): `[V13]`
  WARN → FAIL on eight cases as expected; `[V12]` FAIL on seven of nine per path from
  `side_elems` and, on a3/a6a/a6b, D-13's `orientation_length`. **And one defect the old
  verifier hid — MG-15, below.**

  **MG-15 (2026-09-26): overlapping elements — two tets sharing a face on the same side of it.**
  Found by §4.2's `side_elems` rule and now `[V3]`'s `V3.folded_face`. Default path: a3 466,
  a4 1,080, a6a 1,208, a6b 2,062, a7a 1,440, a7b 1,156, a8 8,928 (a1, a2 zero); gated: a1 952,
  a3 1,382, a4 616, a6a 132, a6b 36, a7a 158, a8 1,766 (a2, a7b zero). Charged to the arm with
  `plc_path`: **every one** is inside a single parent cell between two fan tets — `plc_path 3`
  (§7.6's pieces) on the default path, `plc_path 4` (the facet-split fan) on the gated path;
  §7.4's own output (`plc_path 1`) carries none. Relative apex heights are 0.1–2 edge lengths
  and a3's tets sum to `1 + 3.4e-6` of the box, so this is overlap, not rounding. Mechanism,
  stated as a hypothesis to be measured first: a piece that is not star-shaped from its
  centroid fans into inverted tets, and `orient_positively` (`cut.rs:978`) swaps two nodes and
  emits them positive and overlapping. Seen in M-1.9's renders at a3's fold location as
  overlapping sliver sheets. **Evidence for the mechanism (2026-09-26):** of the folded faces,
  a3 461 of 466, a7a 1,440 of 1,440 and gated a1 952 of 952 contain a node the cut interned — the
  fold face is a spoke of a fan from an interned apex, i.e. two adjacent base triangles seen from
  the apex fold back to the same side: the piece is not star-shaped from its apex. Consistent
  with the hypothesis; the per-tet proof (orientation before `orient_positively`) is M-2's first
  measurement. **Fixed 2026-09-26 (`b61c1fc`), ahead of M-2 because every later `[V3]`
  acceptance depended on it:** both fans now wind the piece consistently and require every face to
  take one orientation sign against the centre, declining the split otherwise (the whole-cell fan
  of a convex lattice cell is always valid). a3: 466 → 0 folded faces (default), 1,382 → 0
  (gated). The cost is where the gated path now declines: a3's containment 97.16 → 94.77 %. A
  fan from one of the piece's own corners was tried and refuted (12 folds, an inverted tet).
  Recovering the declined pieces without a fan - a constrained tetrahedralisation of a non-star
  piece - is **M-2**'s — no fan may emit a tet the fan's precondition does
  not support, and M-3 cannot read "not worse" on `[V3]` until both paths are at zero.
- **M-1.1. The matrix harness.** `run_acceptance.py` gains: a path selector that exists only until
  M-3 deletes it (the env var is passed through, never defaulted), retention of the mesher log and
  the verify JSON per case per path under `data/output/acceptance/<path>/`, wall time per stage
  (`RUSTMSPT_TIME_STAGES` is already print-only), and the reference cases folded in from
  `run_reference.py` so one command produces §2.2 and §2.3 together. `escalation_census.py` is
  extended to charge `[V4]` (below-10° share, worst AR, needle-face inheritance) and `[V6]`/`[V9]`
  residuals to `plc_path`, as `plc_path` already charges `[V13]`.
  *Acceptance:* one command reproduces §2.2, §2.3 and §2.4 of this document to the digit at
  `0a8eb1c`. *Tier T2. Multimodal: no.*
  **Status 2026-09-26 — first half landed.** `run_acceptance.py --path default|gated` (the
  gated env var is set only when named, and removed otherwise), per-path directories under
  `data/output/acceptance/<path>/` keeping `<case>.mesh.log` and `<case>.verify.log`, a timing
  table parsed from `[STAGE-TIME]`, `--focus` running R9's comparison renders per case, and the P3
  table's `cont%`/`cover%`. Still open: folding the reference cases in (with M-1.8, which
  rewrites their resolution mapping) and extending `escalation_census.py` to `[V4]`/`[V6]`/`[V9]`.
- **M-1.2. Byte-identity on the matrix (R-P2).** Both paths, all twelve cases, `RAYON_NUM_THREADS`
  1 vs 8, primary and contract file hashes. a1 gated is measured identical today (§2.6); the rest
  is asserted, and the a8 lesson applies.
  *Acceptance:* a committed table of hashes; any mismatch is a FAIL gate blocking M-3. *Tier T1.
  Multimodal: no.*
  **Status 2026-09-28 — nine of twelve.** `data/fixtures/meshgen/acceptance/check_determinism.py`
  re-runs each case's own config at `RAYON_NUM_THREADS=1` under the runner's environment and
  compares SHA-256 of the delivered volume and the contract document, excluding only the lines that
  name the run (config hash, output paths). On the `acceptance_m28` matrix: 9 / 9 byte-identical on
  both paths, tables in `determinism_gated.txt` / `determinism_default.txt`; the three reference
  cases, both paths, identical as well (`determinism_reference.txt`). M-1.2 done.
- **M-1.3. Time budgets.** Per-case wall time at `0a8eb1c` on both paths becomes the budget line
  in §2.6; a phase that exceeds a case's budget by more than 25 % states why and gets the owner's
  decision before landing. The number 25 % is a reporting threshold, not a mesh setting.
  *Acceptance:* the table exists, with the host identified. *Tier T1. Multimodal: no.*
  **Status 2026-09-28 — the budget line (landed).** Per-case mesher wall time, seconds, at `b9aefe5`
  on host *WSL2 aarch64, 8 × Cortex-X1C, 19 GB*, uncontended (`data/output/acceptance_m30`, the
  runner's own `total` column; verifier time excluded):

  | case | a1 | a2 | a3 | a4 | a6a | a6b | a7a | a7b | a8 |
  |---|---|---|---|---|---|---|---|---|---|
  | default | 0.4 | 1.0 | 3.2 | 15.2 | 1.9 | 2.3 | 2.6 | 2.9 | 108.4 |
  | gated | 3.4 | 2.9 | 16.6 | 83.8 | 6.7 | 9.2 | 8.0 | 8.7 | 125.1 |

  A later phase exceeding its case's gated budget by more than 25 % states why and asks the owner
  before landing. The reference cases are not in this table yet.
- **M-1.4. The P2 baseline at equal fidelity.** For each case, the shipped path's element count at
  the `h` that reaches the gated path's on-surface share (record §6.50's method, `h` halved until
  the share is met or the case exceeds 2 GB), beside the gated path's count at its own `h`. This is
  the number R2 gates against from here on.
  *Acceptance:* the table, with the sweep script committed. *Tier T2. Multimodal: no.*
  **Status 2026-09-28 — landed.** `data/fixtures/meshgen/acceptance/p2_equal_fidelity.py`
  (halves `h_max_frac`, `h_min_frac` and `eps_frac` together until the default path's `[V13]`
  on-surface share meets the gated one, the sizing leaf budget is reached, or 2 GB is exceeded);
  table in `p2_equal_fidelity.md`, at `b9aefe5`. The default path meets the gated share only on a1
  (h/2, 187,626 tets against gated 49,545 - 3.8×) and a7a (h/4, 1,690,573 against 362,723 - 4.7×);
  a3, a4, a6a, a6b and a7b hit the 1,000,000-leaf sizing budget first (a3 98.03 % at 2.08 M tets
  against gated 98.51 % at 251 k; a6a *falls* with refinement, 99.47 → 99.03 %), and a8 reaches
  2 GB at h/4 six points short. So at equal fidelity the gated path is the cheaper one by 3.8× or
  more wherever the comparison can be made at all; R2 gates against the gated counts.
- **M-1.5. The P3 criterion (MG-02).** `[V13]`'s corner test is necessary and not sufficient:
  `good_cube.vtu` verified against a `[0,1]³` cube STL reports `on_surface_area_frac = 1.0`,
  `deviation_max = 0`, `chord_max = 0.5` — its two interior interface triangles run through the cube
  with every corner on its surface. Three points on a piecewise-planar surface do not put the
  triangle they span on it, and an STL is piecewise planar, so "a chord across a crease is the
  price of flat elements" is not a premise; carrying the crease makes the face exact. The criterion
  P3 needs is **containment**: every material-boundary triangle lies in the union of the effective
  input's facets (D-6) — verifiably, from a source the mesh names. Design: per boundary face, the
  arranged patch(es) it claims (the tag, or for an undeclared face the patch `[V13]` locates);
  coplanarity of each corner with that patch's plane, tested with the exact predicate at the
  contract's anchoring tolerance; coverage by clipping the triangle against the patch's own
  triangles in the patch plane (2-D overlay, exact predicates) and requiring the clipped area to
  equal the triangle's; a face no patch covers is a FAIL, a missing boundary is a FAIL, and
  SKIPPED is never returned for a check the stage requires. The corner metrics and `chord_*` stay
  as diagnostics. Inactive patches hidden by priority (R-A4) are legitimate absences and the
  criterion must know them. *Acceptance:* the MG-02 counterexample FAILs; re-triangulating the same
  coplanar patch PASSes; a missing component, a missing open sheet, a chord across an input crease,
  and a priority-hidden inactive patch each have a fixture with the expected code in contracts §6;
  the §2.2/§2.3 on-surface columns are re-measured under the new criterion and recorded beside the
  old — the old values are not inherited as a baseline. *Tier T3. Multimodal: no.*
  **Status 2026-09-26 — landed.** `[V13]` measures containment both ways (contracts rev 1.3.1,
  D-12 closed, D-26 records three departures: against the input triangles, sampled coverage, a
  crease-crossing face fails whole). The MG-02 cube now reads `contained_area_frac = 0` and FAILs;
  the six fixtures are unit tests in `verify.rs`; `run_acceptance.py`'s P3 table leads with
  `cont%` and `cover%`. **Re-measured at the current tree**, containment / input coverage /
  the old corner share:

  | case | default path | gated path |
  |---|---|---|
  | a1 | **0.97** / 6.23 / 92.52 | 99.73 / 99.88 / 99.79 |
  | a2 | 100 / 100 / 100 | 100 / 100 / 100 |
  | a3 | **51.73** / 43.46 / 92.78 | 97.16 / 93.03 / 97.21 |
  | a4 | **52.69** / 58.77 / 98.82 | 99.03 / 99.68 / 99.09 |
  | a6a | 99.32 / 93.83 / 99.54 | 99.94 / 99.63 / 99.95 |
  | a6b | 99.35 / 92.57 / 99.44 | 99.96 / 99.53 / 99.96 |
  | a7a | 98.76 / 87.23 / 99.83 | 99.97 / 100 / 99.96 |
  | a7b | 99.32 / 92.90 / 99.89 | 99.99 / 100 / 99.98 |
  | a8 | 88.09 / 53.71 / 92.04 | (running) |

  **What this changes.** On the default path the corner share over-states P3 by up to 92 points:
  on every curved or intersecting input most of the boundary chords across the STL's own facet
  creases (a1's 1,280 facets: 2,476 of 2,644 faces not contained). On the gated path containment
  and the corner share agree to within 0.1 point — §7.4 takes the facets as constraints and
  carries their creases, which is exactly what containment asks. The default path's P3 case is
  therefore weaker than §2.2 recorded, and M-3's "gated not worse on every FAIL gate" is, on P3,
  already true by a wide margin; what still stands between the gated path and M-3 is MG-15 and
  the `[V2]`/`[V9]` rows. The thin cases' input coverage (a7a 87 % default) is the collapsed gap
  walls, D-8's known departure.

- **M-1.6. Conditioning and identity: the one-ULP pairs (was M-4.1; moved here so M-3 has no
  cycle, MG-12; re-scoped under MG-11).** The `[V2]` pairs are as §2.2 records: a3 19 default / 3
  gated, a6b 1, a8 6, every pair two trace endpoints one float32 ULP apart where a lattice plane
  nearly coincides with an input plane (record §6.60, §6.64). v3's rule (a) — quantise the input to
  `q` at S0 — is refuted on paper before being built: (i) S0 merges by `NodeKey` and keeps the
  first vertex's raw coordinates (`surface.rs`), so "the invariant mesh nodes already obey" is not
  one mesh nodes obey — quantised keys are *identity*, not geometry, and Rule K-O already admits
  constructed points off the grid; (ii) the q grid is not the lattice grid: an input plane on a
  lattice plane at `x = 0.5` is `0.5/√3` in the normalized frame and, rounded to `q = 1e-5` and
  mapped back, becomes `0.5000084271289835` — off the lattice, which breaks the alignment D-3
  keeps and a2 depends on; (iii) `q/2 = 5e-6` is a normalized length and `[V13]`'s `0.02` is a
  fraction of the local edge, so the "four-thousand-times" margin is `(q/2)/(0.02·h_local)` and
  vanishes on a small cut face; the 3-D per-axis rounding bound is `√3·q/2`. D-3 stands: the
  lattice stays aligned with the input, and rule (b), the offset, is not taken. What replaces (a) is
  first a definition and then a measured rule. Numerics rev 1.3 §1.5 freezes the four roles —
  identity key, ordering key, geometric coordinate, error metric — and says which operations may
  change which. The rule for two constructed points one ULP apart is then chosen by measurement on
  the a3/a6b/a8 pairs from candidates that all satisfy: (1) an input plane on a lattice plane stays
  exactly on it; (2) no vertex leaves its own input facet; (3) two entities distinct in the
  arrangement are never merged; (4) every moved or merged point is reported. Candidates: identity at
  S7 by the crossing's *provenance* — `(edge, patch)` — so two spellings of one crossing share a node
  without either moving (the registry principle of numerics §5, one stage later); a `[V2]` duplicate
  tolerance stated against the local edge with the pair kept distinct; input quantisation restricted
  to motion within the vertex's own facet plane. Not a knob. *Acceptance:* `[V2]` PASS on all twelve
  on the one path; `[V3]` and `[V13]` unchanged beyond `1e-5` relative; a2's element count unchanged
  (alignment preserved); every moved or merged point in the run report. *Tier T3. Multimodal: no.*
  **Status 2026-09-27 — the root cause landed in S2, not S0.** a3's gated `[V9]` nodes sat at x =
  0.5000084271289835 - the number (ii) above uses to refute quantisation at S0. It was already
  happening one stage later: `build_registry_and_nodes` placed a constructed intersection point at
  its key's grid coordinates, so every such point left its own facet by up to half a step
  (criteria 1 and 2 above). The key is now identity only; a key group takes its first member's
  constructed point in canonical provenance order. Full matrix (`data/output/acceptance_m27`),
  gated: a3 97.11 → **98.51**, a6a 99.924 → **99.953** at **−37 % tets**, a6b 99.893 → 99.904 at
  −10 %, a8 98.445 → **98.894** at −7.8 %, the rest equal; default: a3 95.09 → 93.77, a6a 99.61 →
  99.47, a8 91.70 → 91.87. a2's element count unchanged (alignment preserved); `[V2]` PASS on all
  nine, both paths; R-P2 held on a3. Two S8 consequences fixed with it: a cap whose carrying tet is
  flat is tagged as the surviving faces in its plane, and `refresh_split_interfaces` re-splits an
  interface face after the edge repair (a3 `[V12]`). Still open for M-1.6: the "every moved or
  merged point reported" clause and the default path's a3/a6a regression (not pursued - M-3 retires
  that path).
- **M-1.7. The X-pattern lattice: the 5-tet parity checkerboard (R-E5, the owner's requirement of
  2026-09-25; D-9, decided the same day; S-53).** Every plain leaf instantiates geometry §2.2's
  table with the same `v0–v7` main diagonal (`lattice.rs:54–61`, applied with no reference to the
  leaf's position at `lattice.rs:706–716`), and every unsplit face takes Rule D, minimum corner to
  maximum corner (`case_plain`, `lattice.rs:409–433`). So every face diagonal of a plane family is
  parallel and every body diagonal is `(1,1,1)`: on a uniform 6³ block, 252 face diagonals per
  plane family all in one orientation and 216 body diagonals all in one direction (Appendix D.1).
  The owner's requirement is the pattern the reference tool's background shows — on every axis
  plane, the diagonals of each 2×2 block of faces meet at its centre — and D-9 chose the reference
  tool's own construction for it. **Design:**
  (a) *Plain leaves.* A leaf of side `s` with origin `(a, b, c)·s` in the doubled index space is
  split into five tets: a central tet on the four corners whose own-level index sum is even, and
  one corner tet at each of the other four corners. For `a + b + c` even the rows are Appendix
  D.1's `FIVE` — central `(v0, v3, v5, v6)`, corners `(v1; v0, v3, v5)`, `(v2; v0, v6, v3)`,
  `(v4; v0, v5, v6)`, `(v7; v3, v6, v5)`; for `a + b + c` odd the same rows relabelled
  `v_i → v_(i ⊕ 1)` — each under the canonical orientation fix (§1.1). The central tet is regular,
  edge `√2·s`, volume `s³/3`; each corner tet is a right-corner tet of volume `s³/6`; the five close
  the cube exactly. A plain leaf has no body diagonal and no interior node.
  (b) *Rule T5 replaces Rule D* in §3.3's case P, and therefore in case Q at level `L+1`: on an
  unsplit axis-aligned face of side `s`, the diagonal joins the two corners whose own-level index
  sum — normal coordinate included — is even. It is a pure function of the face's four corners and
  its own side, exact in integers and independent of the calling cell and its winding (Invariant
  C); no `NodeKey` order and no node index is consulted. A plain leaf's faces obey it by
  construction (each face holds exactly two of the leaf's even-sum corners, diagonally opposite),
  so T1's proof (§3.6) holds verbatim with Rule T5 in place of Rule D in its cases (i) and (ii).
  Because the normal coordinate enters the parity, the X on one axis plane sits one cell over from
  the X on the next parallel plane — the reference tool's own pattern.
  (c) *What follows, machine-checked (Appendix D.1).* On a uniform block the face diagonals split
  evenly between the two orientations of each plane family (126/126) and there is no body
  diagonal. Plain leaves emit **five tets instead of six** (1,080 against 1,296 on the 6³ block,
  −16.7 %), and interior vertex valence is 18 at even-sum points and 6 at odd-sum points (mean 12,
  from 14). Quality: the plain-leaf row goes from `45.000° / 90.000° / AR 1.3938` to
  `54.736° / 90.000° / 1.3660` (the corner tets; the central tet is regular). Coarse faces lie on
  even planes at level `L+1`, so case Q's quadrant diagonals all run through the coarse face's
  centre and every fan tet over a Q triangle is a Kuhn simplex: `35.264° / 125.264° / 1.6052` →
  `45.000° / 90.000° / 1.3938`. The P and E fans are unchanged, so the lattice-wide worst becomes
  `45.000°` minimum and `120.000°` maximum dihedral at `AR 1.5607` (the P-face fan). Conformity:
  zero unpaired faces, zero hanging nodes and exact volume on seven strongly balanced
  configurations — uniform, one refined cell, face-, edge- and vertex-only neighbours, a refined
  centre, three levels — while the unbalanced negative control (a level-2 leaf touching level 0)
  fails, as it must.
  (d) *Scope in the code.* The plain-cell loop emits the checkerboard rows by the leaf's index-sum
  parity instead of `FREUDENTHAL`; `case_plain` takes the face's side and applies Rule T5; the
  pre-emission budget `6·N_Freudenthal + 48·N_fan` (§3.5, `LATTICE_MAX_TETS`) becomes
  `5·N_plain + 48·N_fan`; `CellTemplate::Freudenthal` is renamed for what it now holds. No other
  production code reads the lattice's diagonal: `verify.rs`'s `KUHN` table (`verify.rs:5049`)
  builds a `[V13]` test fixture, and Rule SNK in `cut.rs`/`thin.rs` orders cut quads, a different
  rule.
  (e) *Spec.* Geometry rev 1.7: §2.2 gains the checkerboard table (both parities, positively
  oriented, with volumes) as the plain-leaf template, the Freudenthal table kept as history; §2.3
  Rule T5 with its rationale corrected (S-53); §3.3's cases P and Q; §3.4's Bound P1 becomes
  `h³/48 ≤ V ≤ h³/3` (the central tet); §3.5's plain row (6 P faces, 5 tets, `h³/6 … h³/3`) and
  its budget; §3.6's proof text; §3.7 re-measured with the plain row, the Q row and a lattice-wide
  worst line; §13 the renamed tests; §14 two independent confirmations (R6) — Appendix D.1's
  enumeration and the shipped `[V4]` on a real graded lattice agreeing to every printed digit, the
  way rev 1.2's Q-row correction was confirmed; §15 the deviation row.
  (f) *Tests.* `the_frozen_freudenthal_table_is_positive_and_tiles_the_cube` (`lattice.rs:972–983`)
  becomes the checkerboard's: both parities positive, volumes `s³/3 + 4·s³/6`, exact tiling;
  `rule_d_reproduces_the_frozen_face_table` and `rule_d_is_invariant_under_rotation_of_the_quad`
  (`lattice.rs:985–1056`) become their Rule T5 equivalents, the invariance now under rotation
  **and reversal** of the quad (two cells walk a shared face in opposite directions — the face
  cache's lesson), plus a test that both parities' face traces obey Rule T5;
  `a_uniform_lattice_is_all_freudenthal_and_tiles_the_domain_exactly` and
  `a_graded_lattice_uses_fan_cells_and_stays_within_the_template_inventory`
  (`tests/meshgen_lattice_tests.rs:203–264`) move to five tets per plain leaf and the
  `[h³/48, h³/3]` volume range; `the_templates_match_the_corrected_quality_table` (281–314) moves
  to `45.000°` / `1.5607`; `FAN_BASELINE_DEG` (`tests/meshgen_quality_gate_tests.rs:28`) moves to
  `45.0` and the gate's plain-cell reference from `45.000°` to `54.736°`, both comments re-derived
  from Rule T5; a uniform-block test asserts the edge-direction histogram is symmetric under the
  three reflections and that no body diagonal exists. T1's randomized, edge-only and non-cubic
  conformity tests pass **unedited** — they are the conformity proof.
  (g) *The matrix, after M-1.8.* Every mesh changes, and not by a relabelling: the tets S7 snaps
  into and S8 cuts change **shape** wherever the lattice is plain — a regular central tet of twice
  the corner tets' volume, no body diagonal, one lattice tet in six fewer. So the nine cases and the
  three reference cases are re-run on both S8 paths and recorded as new §2.2/§2.3 rows beside
  M-1.8's, each moved number charged to this change, element counts included. The lattice's
  diagonal facet planes change too: Rule D's lie on `x_i − x_j ∈ ℤ·s`, while the checkerboard
  carries none of that family — its interior facets lie on `±x ± y ± z ∈ 2ℤ·s` (Appendix D.1,
  enumerated on the uniform block) — which moves record §6.64's coincidence class (a3's cube edge
  on the Rule-D plane `x − y = 0.5`): M-1.6 counts that population after this subtask, not before.
  *Acceptance:* the X pattern on every axis plane of every case, shown by a front-view wireframe of
  each case's box face — the view of the owner's screenshot of 2026-09-25 — beside the reference
  tool's own `*_final.vtu` rendered the same way, plus a cutaway through a transition region;
  `[V1]`/`[V3]` clean on the matrix, both paths; the pre-cut lattice's `[V4]` minimum dihedral
  `45.000°` on a graded lattice and `54.736°` on a uniform one; T1's tests green unedited; R-P2
  byte-identity on a1 and a8; no FAIL gate worse than the M-1.8 row; every row whose element count
  or on-surface share moves by more than 2 % explained before landing (a reporting threshold, not
  a gate), with the element-count change charged separately to the plain-leaf template; reference
  docs, en-us and zh-cn, updated in the same change. The declined alternative — the
  parity-reflected Freudenthal lattice (Rule X: six tets per leaf, mirrored by index parity) —
  keeps its rows in Appendix D.1 for the record. *Tier T2 (the lattice) / T3 (geometry rev 1.7 and
  its verification). Multimodal: yes — the box-face wireframes set against the reference tool's
  own mesh, and the transition cutaways; the requirement is a visual one.*
  **Status 2026-09-26 — landed (`17e5a9d`).** `CHECKERBOARD` and Rule T5 (`case_plain`) in
  `lattice.rs`, `CellTemplate::Plain`, budget `5·N_plain + 48·N_fan`; geometry rev 1.7 (§2.2–§2.4,
  §3.4–§3.7, §14 [12], D-44). T1's tests pass unedited; four new unit tests and a uniform-block
  symmetry test; the quality gate reads plain `54.736°`, fan `45.000°` pre-snap exactly as (c)
  predicted. **Visual acceptance (R9):** the box face of TestCaseIntersect1 at 17³ shows the same X
  pattern as the reference tool's own `test_mesh_final.vtu` where the old lattice drew one
  diagonal direction everywhere, and a z = 0.5 cutaway through a3's refined region shows the X on
  both levels and across the transition with no T-junction (`data/output/m17/front3.png`,
  `cut2.png`). R-P2: a1's s05/s08 are identical at 1 and 8 threads. Reference case 1 at its own
  resolution: 153,636 → 131,396 tets (0.43× → 0.37× the reference), corner share 93.07 → 95.24 %.
  The nine-case matrix on both paths is being re-run against M-1.8's row.
- **M-1.8. Resolution as a background and a maximum level — the ladder, given (R-E4, the owner's
  requirement of 2026-09-25; S-54; §6.2 item 18).** Today the bounds are two fractions of the
  domain diagonal, `sizing.h_max_frac` and `sizing.h_min_frac` (`src/config/meshgen.rs:106–122`),
  and the octree realises them on a ladder anchored at the domain's **longest axis**: one root cube
  of that side (`sizing.rs:1417–1418`), the background at the first halving `≤ h_max`
  (`forest_level`, 1425–1430), the finest at the first halving `≤ h_min` (1419–1424), at most
  `SIZING_MAX_LEVEL = 12` levels below the root. So the background is a power-of-two count along
  the longest axis — 16 or 32, never 17, 24 or 45 — the number of levels is the by-product of two
  roundings, and a uniform lattice is rejected (`h_min < h_max`). **The keys:**

  ```yaml
  meshgen:
    sizing:
      background: {cells: [24, 24, 30]}   # | {cells: 20} | {size: 0.0167} | {size_frac: 0.029} | auto (M-4.6)
      max_level: 2                        # | 0 | 1 | ... | auto (M-4.6)
    inputs:
      - stl: particle.stl
        max_level: 2                      # optional: this input's own sources refine at most this deep
  ```

  `auto` is the derived half and lands with M-4.6; until then it is a parse-time reject that says
  derived resolution is not implemented yet, and a config that gives only one of the two keys is
  rejected the same way.

  (a) *The ladder.* Background cell edge `h_bg`; finest edge `h_min = h_bg / 2^L`. The octree root
  becomes `h_bg · 2^m`, anchored at the domain's minimum corner, with `n_i = ⌈E_i / h_bg⌉` the
  realised background counts over the extents `E_i` and `m = ⌈log₂ max_i n_i⌉`; the background is
  octree level `m` (the `forest_level`) and the finest level `m + L`, both exact because only
  powers of two scale `h_bg`. D-38's whole-subtree trimming at levels `≤ forest_level` then cuts
  the root down to exactly `n_x × n_y × n_z` background cells — the mechanism a non-cubic domain
  already relies on, so T1 and L1 hold by the argument that covers the trimmed boundary today. The
  lattice overhangs the box only where an extent is not a multiple of `h_bg`, by less than one
  background cell, as today. Counts and the outside test are taken with the weld quantum as
  tolerance (`0.6 / 0.05` is 12, not 13). The drop predicate (`sizing.rs:1451–1457`) already
  counts a cell that only touches a box face as outside (`min ≥ domain_max`), but on raw floats:
  with the root on the longest axis it has never met an exact fit on a shorter one, and
  `12 · size` one ulp below `domain_max` would keep a thirteenth layer. `SIZING_MAX_LEVEL` is restated as a cap on `L` plus the index-width condition
  `m + L + 1 ≤ 31` (the doubled index space is `u32`); D-38's "levels below 2 are never split by
  balance" is restated relative to the background level, or shown to hold when `m < 2`;
  `SIZING_MAX_LEAVES` and `LATTICE_MAX_TETS` are unchanged and are checked **pre-flight** from the
  ladder, with the predicted counts reported.
  (b) *Background forms.* Cells are cubic — T1's quality table, the isotropic sizing field and
  every `h`-relative threshold assume it; anisotropic backgrounds are out of scope. `cells: n` is
  `n` cells along the longest axis. `cells: [nx, ny, nz]` sets `h_bg = max_i E_i / n_i` and is
  accepted iff `⌈E_i / h_bg⌉ = n_i` on every axis — the one choice that honours every count exactly
  whenever a cubic size can (all three reference backgrounds qualify, cubic to `3e-12`); otherwise
  a parse-time reject names the consistent triples on either side and their `size`. `size` is in
  model units, `size_frac` a fraction of the domain diagonal (the unit of today's fractions).
  Nothing is re-interpreted silently: realised counts, `h_bg` and the overhang per axis are printed.
  (c) *Level.* `L ≥ 0`, and `L = 0` — a uniform lattice, the reference's level 0 — is legal: the
  `h_min < h_max` reject is replaced by the level cap. `L` is validated against the envelope rule
  on the realised finest edge (`eps_frac < 0.5 · t_sheet_factor · h_min / diag`, today's `eps_cap`
  at `src/config/meshgen.rs:357–364`) and against the cap. `inputs[].max_level ≤ L` caps the
  refinement that input's own sources may request; a source owned by several inputs — an
  intersection curve, a gap between two of them — takes the finest of its owners' levels. That is
  what the reference's per-section level means, and TestCaseIntersect3 needs it (level 2 on eight
  sections, 0 on the ninth). Per-input *backgrounds* are not in scope.
  (d) *What the bounds feed.* Every consumer of `h_max`/`h_min` is enumerated from the code —
  `clamp_h`; the curve target `h_max / curve_cells`; `lfs_floor = max(ε, gap_cells · h_min)`; the
  S3↔S4 start `h⁽⁰⁾ = h_max` (S-33) and floor; K1's clamp; `eps_cap`; whatever else the survey
  finds — and each reads `h_bg` or `h_bg / 2^L` under the new keys; the list goes into the
  reference doc.
  (e) *A given value is a cost cap, never a licence.* It is honoured exactly. Where it is below
  what a feature needs, the mesh is still built at it, the report names the feature, and the gates
  report the consequence — P1 as restated in §1.3: a named, reported condition, never a silent
  degradation. R3 is not engaged: the ladder chooses between meshes of different resolution, every
  stage still decides per cell inside it, and no mechanism is switched off.
  (f) *The fractions.* `background`/`max_level` and `h_max_frac`/`h_min_frac` are mutually
  exclusive, a parse-time reject naming both. With the new keys absent the pipeline is unchanged
  to the byte; M-4.6 retires the fractions once it has migrated the acceptance configs.
  (g) *Report.* One `[S4/RES]` line: each quantity given or derived, `h_bg` in model units and as a
  fraction, realised counts and overhang, `L`, `h_min`, octree levels `m` and `m + L`, per-input
  levels, predicted leaves and tets against their budgets. The same values are stamped in the
  snapshot's FieldData, listed as non-contract until contracts rev 1.4 freezes them (§8).
  (h) *The reference cases at their own resolution.* `run_reference.py` reads `*RVE_CORNERS`,
  `*NUM_ELEMENT` and every section's `*SAMR_LEVEL` from the reference inputs and writes
  `background: {cells: [...]}`, `max_level:` the largest section level and `inputs[].max_level:`
  per section, in place of `h_max = Base Mesh Element size` and the first `SAMR levels:` line.
  §2.3 is re-measured on both paths and recorded beside the old rows, and A-10 and §2.5's P2
  verdict are re-read from the new ones.
  *Acceptance:* the nine cases unchanged to the byte with the new keys absent; for each case, the
  keys stating today's realised ladder (`h_bg = E_max / 2^forest_level`, `L = max_level −
  forest_level`) build the identical octree, and any mesh difference is charged to a named consumer
  in (d); the three reference cases realise the reference's background counts exactly and its
  per-section finest edges (`reference_resolution.py`, Appendix D.2, prints both); on a
  `[0,1]×[0,1]×[0,0.6]` box `cells: [20, 20, 12]` leaves no overhang, `[V1]`/`[V3]` clean and the
  tet volume equal to the box's, while `cells: [20, 20, 10]` is rejected naming `[20, 20, 12]` and
  `[17, 17, 10]`; a 17³ background at `L = 2` and a uniform `L = 0` lattice mesh with `[V3]` clean;
  one negative config fixture per reject (both forms given, an inconsistent triple, the level cap,
  the envelope rule, an input level above the global one); reference docs, en-us and zh-cn,
  updated in the same change. *Tier T2. Multimodal: yes (R9) — M-1.9's set on the nine cases,
  unchanged with the keys absent; on the reference cases, the particle-contact curves before and
  after the ladder moves.*
  **Status 2026-09-26 — landed (`b61c1fc`, `d0c6985`).** `sizing.background` / `sizing.max_level`
  / `inputs[].max_level` in `config/meshgen.rs` (`MeshGenParams::resolution`), the root at
  `h_bg·2^m` (`SizingOptions::root_size`) with a touch tolerance only under the ladder, per-input
  floors (`SizingOptions::component_floor`, input `i` = component `i + 1`, a shared source takes
  the finest floor), `[S4/RES]`. Verified: with the keys absent a3's s05 is byte-identical apart
  from `ConfigHash`; stating a3's realised ladder (`cells: 16`, `L = 2`) gives the same mesh as
  fractions equal to its `h_max`/`h_min` — the difference from the legacy fractions is entirely
  the two consumers that read `h_min` (`lfs_floor`, 51,407 → 76,560 LFS sources) and `h_max`
  (the curve target), which (d) anticipates; `[20, 20, 12]` on a 1×1×0.6 box tiles with zero
  overhang, `[20, 20, 10]` is rejected naming `[20, 20, 12]` and `[17, 17, 10]`, 17³ at `L = 2`
  and a uniform `L = 0` build, all with no leak, no hanging node, no node outside the box and the
  box filled (their only `[V3]` finding was MG-15, since fixed); a cube at level 0 beside a sphere
  at level 2 cuts a3's leaves 13,070 → 12,342 with the intersection curve still at level 2.
  `run_reference.py` reads `*RVE_CORNERS`/`*NUM_ELEMENT`/`*SAMR_LEVEL` and maps each section's
  level onto its `particleN.stl`. Eight negative/positive config tests. Open: the §2.3 table on all
  three reference cases, both paths.
  **Status 2026-09-28 — the reference cases at their own resolution, both paths.** `run_reference.py`
  at `b9aefe5` (`data/output/reference_m30`), each case at the reference run's background and
  per-section levels:

  | case | reference tets | default tets (ratio) | default on % | gated tets (ratio) | gated on % |
  |---|---|---|---|---|---|
  | TestCaseIntersect1 | 354,372 | 131,410 (0.37×) | 95.248 | 341,696 (0.96×) | 93.673 |
  | TestCaseIntersect2 | 730,199 | 275,226 (0.38×) | 96.097 | 541,025 (0.74×) | 96.538 |
  | TestCaseIntersect3 | 1,167,239 | 1,094,214 (0.94×) | 96.831 | 1,565,171 (1.34×) | 99.549 |

  FAIL sets after this session's two follow-up fixes: default {V6, V13} / {V6, V13} / {V13}; gated
  {V6, V13} / {V6, V13} / {V9, V13}. Gated case 3's one `[V9]` node sits on the domain face in two
  whole-cell-fanned cells (M-2.3's fallback), and gated case 1's on-surface share is 1.6 points
  *below* the default path's - the one case where M-3.1's `[V13]` clause fails.
- **M-1.9. The focus-region render harness (R9, the owner's instruction of 2026-09-26).**
  `data/fixtures/meshgen/acceptance/render_focus.py`, run on a case's output directory: reads the
  s02 arranged snapshot's curve cells (`CurveKind` 0 sharp, 2 intersection; box curves skipped),
  S3's thin regions from s03, and level jumps from s05; picks sample points deterministically —
  every point where three or more curves meet (corners, triple points), then points at fixed
  arc-length fractions, capped per class so a8's 1,404 curves give a bounded set; writes one
  `mesh-render` config per view (a `bbox` window of a few local `h` around the point, and a
  `clip_plane` through it with the curve tangent as its normal, `wireframe: true`, `color_by:
  region_key`, `show_faces: false`, curve cells on, 2048 px, orthographic, the camera fixed by the
  point and the tangent); renders on the CPU path (the transparency reference); assembles one
  labelled contact sheet per case and a manifest (view name → point, tangent, curve id, kind); and
  with `--before DIR` writes per-view changed-pixel shares and a diff image. The acceptance runner
  calls it after each case.
  *Acceptance:* sheets for all nine cases at the current mesher, taken as the "before" of every
  later subtask; the harness finds the defects already known to be there — a6a's limb-edge
  serration, a3's chamfer where the sphere crosses the cube — or it is showing the wrong places
  (R8 applied to a picture); two runs give byte-identical PNGs; a view count per case printed and
  bounded. *Tier T2. Multimodal: yes — the sheets themselves, and the two known defects.*
  **Status 2026-09-26 — redesigned on the owner's review, landed.** The first version (sections
  of the cut mesh coloured by region) was rejected by the owner as showing nothing: a picture of
  the mesh alone cannot say what the defect is, where, or what it looks like. The rule it now
  follows, and every later render must follow: **set the input STL beside the output, from the
  same camera, and colour the output by how far it is from the input.** `mesh-verify` gained
  `fidelity_vtu:`, which writes the material boundary [V13] measured (per face: corner deviation
  over the face's edge, signed offset, and a traffic-light class — grey < 2 %, yellow 2–10 %,
  orange 10–25 %, red ≥ 25 %) beside the input triangles (split to the output's face size so a
  window cannot drop a two-triangle cube face), in one surface document. `render_focus.py`
  renders an overview (input | output, two opposite corners) and the defects: faces ≥ 10 % off
  clustered by location, ranked by off-surface area, each head-on and oblique as input | output |
  semi-transparent overlay, with location, component and worst deviation in `summary.json`.
  `mesh-render` draws element edges on surface-only documents. Renders are byte-identical run to
  run. What a3 shows at once: a row of red/orange teeth along every cube edge where the input has
  a straight sharp edge, and spikes along the sphere–cube intersection curve. Still to land:
  thin-region and lattice-transition views, and the runner calling it.

### M-2 — Kernel completeness: the surface is a union of element faces everywhere

The one phase that changes what a cell becomes. Everything it needs is measured and located (§2.4):
every remaining two-component `[V6]` step, every `[V9]` node and 91.7–98.4 % of every case's
off-surface area sit in the cells §7.4 **refused** (a3 332, a6a 75, a8 3,281), and the refusals are
two classes of recovery. **Facet recovery** — "a facet's edges are all there but its interior is not
covered" (a3 227, a6a 42, a8 411) and "a facet edge is not an edge of the tetrahedralisation" (87,
10, 1,352) — is flips and edge removal only, and 90.4 % of `remove_edge`'s refusals are one line,
*"the link polygon has no valid triangulation"* (record §6.49); the remedy the literature names is
Steiner insertion **on the facet**, and the record shows the alternative, a cone in the link, is
inert (X-7). **Boundary consistency** — "the hull carries a node the boundary has never heard of"
(a8 1,200, a6a 21) and "a boundary node is off the hull" (a8 228) — is a facet vertex reaching the
cell's boundary without being a trace point of that face, where no Steiner point may go (J1).

- **M-2.0. Invariant J1 is a hard error (MG-06).** `check_face_cache` receives
  `FingerprintMismatch` (and a triangle-set disagreement) from `FaceTriCache::get_or_insert`,
  increments a counter and returns `None`; the caller keeps its own triangulation and the run ends
  with one warning line — so once J1 has failed, two owners continue with different triangulations
  of one face, which is the crack J1 exists to prevent (the code's own comment: "counted rather than
  raised"). The fingerprint is also weaker than §7.3 states — the component-id set plus a crease
  flag, not the sorted constraint entity ids — so two owners that disagree about the *same*
  component's trace share a fingerprint and the mismatch is never seen. Fix: the fingerprint is the
  sorted list of the face's constraint entities (trace-point ids per edge, chord endpoints, hub id);
  a mismatch aborts the cut with `Err`, dumping the face key, both constraint sets and both owners;
  the face's constraints are settled once before either owner consumes them (the escalated-cell
  face loop is already sequential — record §6.53's note — so no synchronisation is added). The zero
  conflicts §2 records prove the check never fired, not that its error handling is right.
  *Acceptance:* an injected conflict — same key, different constraint ids; same key, different
  triangle set — makes `cut_lattice` return `Err` and no `s08` is written; a shared face reached with
  reversed winding still hits; the matrix is unchanged to the byte. Lands before M-2.1, which relies
  on J1 being enforced. *Tier T2. Multimodal: no.*
  **Status 2026-09-26 — landed.** `check_face_cache` records the first mismatch - constraint sets
  or triangle sets - with the face key, the corners and both sides in `CutMesh::j1_failure`, and the
  pipeline refuses the mesh before `s08` is written. The fingerprint now carries the face's traced
  nodes (every walk node beyond its three corners) after the component set. Three unit tests
  (`cut::j1_tests`): a constraint mismatch and a triangulation mismatch are recorded; the same
  face walked in reverse still hits. On a3 and a6a, both paths, the strengthened fingerprint
  still reports 0 conflicts (3,463 / 79 / 2,267 / 437 shared faces). Deviation from the text: the
  failure is a field the pipeline turns into `Err`, not `cut_lattice`'s return type, so its eight
  call sites keep their signature.
- **M-2.1. Facet recovery by Steiner points on the constraint — prototype gate.** In
  `constrained_tets`, after edge removal stalls: (a) a facet edge that is not an edge of the
  tetrahedralisation and lies **strictly inside the cell** is split at its exact midpoint
  (quantised to a key, welded within `q`), the facet polygon is split with it, and Delaunay +
  boundary recovery re-run; (b) a facet whose edges are present but whose interior is crossed by a
  mesh edge receives the certified intersection point of that edge with the facet plane (a C1-class
  construction under numerics §4, DD-escalated by the same rule S2 uses), inserted, and recovery
  re-run. Both loops are bounded by the cell's own `tol` (an edge shorter than `tol` is not split;
  a crossing closer than `tol` to a facet vertex snaps to it) — no new number. **Never on a shared
  face** (J1): a facet edge on a cell face is an edge of that face's trace by construction and is
  recovered by boundary recovery, not split. Points are interned through the existing `plc-arena`
  path (`cut.rs:2856–2900`), which already interns only points some tet uses.
  *Measured on:* a3, a6a, a8 gated, then all twelve. *GO if:* the two facet-class refusals fall by
  at least half on each of a3/a6a/a8 with `[V1]`/`[V3]` held and time inside budget; *NO-GO*
  otherwise, recorded with the refusal histogram, and the facet-split fan stays as the only
  fallback.
  *Acceptance (production, together with M-2.2):* whole-cell fans **0** on all twelve; `[V6]` PASS
  on all twelve; `[V9]` PASS on all twelve; `[V13]` on-surface not below the gated value on any case
  and above it where the fan carried area; elements within M-1.4's budget; R-P2 held.
  *Tier T3. Multimodal: yes — cutaway renders (`mesh-render`, clip through the cube's edges on a3
  and a strut junction on a8) before/after, because a junction that meshes but tears is visible
  before it is countable.*
  **Prototype 2026-09-26 — NO-GO, reverted (patch kept as `scratchpad/m21_steiner.patch`).**
  Built as `constrained_tets_with_steiner` (a detailed `constrained_tets` that also returns the
  tetrahedralisation it reached; up to 3 rounds; (a) midpoint splits of missing facet edges not in
  a cell face plane, inserted into every facet carrying the edge; (b) facet-plane crossings of
  mesh edges inside the facet) and wired into `plc_attempt`, on the checkerboard lattice + M-2.0.
  Gated, facet-class declines (edge / interior): **a3 114 / 224 → 60 / 281**, a6a 25 / 48 → 2 / 43.
  The edge move works - it removes half to nearly all of its class - but the cells it frees mostly
  fall to the interior class, and the interior move fired only 35 times on a3: the crossings it
  inserts do not make the interior coverable. a3 containment 93.41 → 93.84 %, `[V1]`/`[V3]` held,
  but a6a gained 2 undeclared material-boundary faces at a Steiner point on the limb edge
  (`[V6]` PASS → FAIL). Eight rounds instead of three ran away (50 CPU-minutes on a3). Next attempt
  should start from the interior class: dump one interior-declined cell (`RUSTMSPT_PLC_DUMP`) and
  read which mesh edge crosses the facet and why flips cannot remove it before choosing a point.
  **Follow-up 2026-09-26 — the interior class was mostly a false refusal; landed.** Dumping every
  interior-declined facet on a3 (`RUSTMSPT_FACET_DIAG`, print-only) found **90 % with no mesh edge
  crossing them at all**: their vertices sit 10¹–10⁶ `tol` off the facet's plane - the rim is
  snapped to the face traces (§6.43), so a facet is planar only to its own snap - and the
  `tol = edge·1e-9` plane test rejected every face of the facet, covered area 0, while the faces a
  little off the plane covered it to the digit. `cdt::facet_plane` gives the facet's band (own
  deviation + `tol`), used by the coverage check (now by projected area), `regions_by_constraint`
  and the interface attribution. Gated: interior class **224 → 42 (a3), 48 → 4 (a6a), 632 → 246
  (a8)**; on-surface 93.77 → **95.64 %**, 99.865 → **99.903 %**, 97.80 → **98.13 %**; containment
  93.41 → 95.35 %, 99.870 → 99.903 %, 97.70 → 98.04 %; input covered 89.96 → 94.22 % on a3;
  `[V6]` a3 undeclared 101 → 85; `[V1]`/`[V3]` held, no new FAIL, R-P2 byte-identical (a3, 1
  thread). Visual (R9, a3): the orange sawtooth along the cube's left and bottom edges is gone;
  what remains is the sawtooth where the sphere meets the cube's x = 0.5 face (up to 129 % of an
  edge) and four spots on the sphere near x ≈ 0.25 - both present before. It also exposed a writer
  bug: `FaceTagOrientation` had one entry per face, not per member (`[V12]` failed on gated a6a
  before this change); fixed. The edge class is untouched (a8 1,835), so M-2.1's GO criterion is
  still unmet and the Steiner prototype stays reverted; the next attempt on the edge class starts
  from a dump, as the interior one did.
  **Re-measured on top of `facet_plane` 2026-09-26 — landed on the gated path; GO gate still
  unmet.** The NO-GO above turned on the interior class, which was mostly the planarity artefact,
  so the prototype was re-applied with two corrections: a midpoint is accepted only when `intern`
  creates a new node (a welded existing node bent the facet - a6a's `[V6]` regression), and a
  refusal restores the facets and truncates the arena (`NodeArena::truncate`), because the
  facet-split fan capped with the collinear midpoints (+32 undeclared faces on a3). Against the
  `facet_plane` baseline: edge / interior refusals **a3 114 / 42 → 49 / 69, a6a 25 / 4 → 2 / 3,
  a8 1,835 / 246 → 1,454 / 273**; on-surface 95.64 → **96.39 %**, 99.903 → **99.924 %**, 98.13 →
  **98.41 %**; containment 95.35 → 96.22, 99.903 → 99.924, 98.04 → 98.30 %; every section keeps
  its status on all three, `[V6]` a3 undeclared 85 unchanged, R-P2 byte-identical (a3), time
  unchanged (a3 15 s, a8 188 s). Visual (R9, a3): no new defect; the x = 0.5 intersection
  sawtooth and the sphere spots remain. It measures strictly better, so it lands; but the gate
  "both facet classes halved on each case" is not met on a8 (−17 %), so the fan is **not**
  deleted (M-2.3 stays blocked). A missing facet edge on a3 has a node within 1e-6 of it in only
  14 of 1,370 cases (`RUSTMSPT_FACET_DIAG`), so the edge class is genuine recovery; a8's 1,454
  is the next dump.
  **M-1.7's cost, measured 2026-09-27, and one part of it repaid.** The two full matrices (pre-
  checkerboard build `b61c1fc`, checkerboard build `d0c6985`+M-1.7) differ by the checkerboard
  alone (M-1.8 was measured byte-neutral). Elements fall 5-17 % on both paths, but gated
  on-surface falls a1 94.89 → 84.34, a3 95.06 → 93.77, a8 99.13 → 97.80 %, a7b 99.98 → 99.78 %,
  and the default path gains `[V9]` FAILs on a3/a6a/a6b. On a1 the decline count barely moves
  (67 → 93) while the area each decline strands grows 2.6× - a 5-tet cube's central tet is a third
  of the cube, so a fanned cell carries more surface - plus one new class, "every hull edge removed
  cleanly and none brought the hull closer" (32 cells, all a planar hexagon on a cell face
  triangulated two ways, needing flips that hold the wrong-face count). `recover_boundary` now
  measures progress as (wrong faces, wrong hull edges) lexicographically: the class is 0, a1
  gated **94.68 %** (with the facet band and Steiner: 84.34 → 90.19 → 94.68), a3 **96.98 %** (above
  the pre-checkerboard 95.06), a8 98.41 %, a6a unchanged; R-P2 byte-identical. Still open from
  M-1.7: the default path's new `[V9]` FAILs (a3, a6a, a6b) and a8/a7b's residual gap to the
  pre-checkerboard gated numbers - to be re-measured on the HEAD matrix
  (`data/output/acceptance_head`) before being charged.
  **HEAD matrix, gated, 2026-09-27 (`a718767`; `data/output/acceptance_head`)**, against the
  pre-checkerboard build: on-surface a1 94.89 → 94.68, a3 95.06 → **97.08**, a4 98.76 → **99.43**,
  a6a 99.919 → 99.924, a6b 99.952 → 99.893, a7a 99.738 → 99.766, a7b 99.983 → 99.778, a8
  99.134 → **98.406**; tets fewer on seven of nine (a1 −14 %, a4 −17 %, a8 −8 %; a2 +19 %, a6b +5 %).
  Section statuses equal to the pre-checkerboard gated column everywhere except `[V9]` on a7a
  (FAIL → PASS). Open: a8's 0.73-point gap, a7b's 0.2 and a6b's 0.06, and the default path's new
  `[V9]` FAILs on a3/a6a/a6b.
  **Composite 2-3 + removal, 2026-09-27.** a8's gap to the old lattice is not the facet classes:
  its stranded area nearly doubled (3.96e-3 → 7.31e-3) and "the link polygon has no valid
  triangulation" went 74 → 1,024 cells, 53 % of it. Dumped (`[HULL-QUAD]`): a strictly convex
  planar quad on a cell face whose hull diagonal carries a fan of 3–4 tets, so `remove_edge`'s
  four-point link cannot be split and the enabling 2-3 flip is never taken alone. The removal is
  now retried after a 2-3 flip on an interior face of the fan. Link class a1 18 → 12, a3 30 → 24,
  a6a 56 → 6, a8 1,024 → 733; on-surface a1 94.68 → 94.80, a3 97.08 → 97.19, a6a 99.924
  unchanged, a8 98.406 → 98.445; statuses unchanged, R-P2 byte-identical. The freed cells mostly
  reach "a tet is thinner than the node quantum" (a8 186 → 438) - the next dump.
  **Thin-tet residue, 2026-09-27 — dumped, one remedy refuted.** `RUSTMSPT_THIN_DIAG`: the thin
  tets `remove_thin_tets` leaves are slivers lying flat on a planar quad of a cell face (2 hull
  faces, 5 boundary edges; a6a 833 of 837), dihedral ~π at the quad's other diagonal, whose ring
  (sliver + 2–4 interior tets) `remove_edge` cannot re-split (425) or re-splits into another
  sliver (383). Re-coning the whole ring from one of its vertices was built and measured **inert**
  (a1/a3/a6a identical to the digit): every apex is either not a star centre of the ring (want/got
  sign mismatch, 2,500+) or yields a new sliver (900+). Reverted; the diagnostic kept. This class
  is ≤ 20 % of a8's stranded area. Not pursued further now.
  **Non-star §7.6 pieces meshed by the constrained kernel (default path), 2026-09-27.** The
  default path's new `[V9]` FAILs (a3/a6a/a6b) are cube corners no body element touches: the
  corner cells escalate `Inconsistent`, the split declines ("a piece does not fan without a flat
  or folded tet", MG-15's guard, or no component crossing at all) and the whole-cell fan labels
  every tet background. `cdt_piece` now meshes a non-star piece from its own nodes with
  `constrained_tets` (volume checked to 1e-9) instead of declining the split: a3 94, a6a 63, a6b
  122, a8 835 pieces. Full matrix (`data/output/acceptance_m23`), default on-surface a3 93.53 →
  **94.96**, a6a 99.47 → 99.61, a6b 99.20 → 99.26, a8 91.06 → **91.95**, the rest equal or up;
  gated (with the composite move) a1 94.80, a3 97.11, a4 99.48, a8 98.45; no status changes,
  tets equal or fewer, R-P2 byte-identical (a3). The `[V9]` corners stay: the corner lies on a
  shared lattice face with no edge crossing, so neither the split nor the kernel has a cut to
  make there - the sub-cell body of record §6.14 in its corner form.
  **Refuted alongside it: fanning a non-star piece from one of its own vertices.** Tried as the
  fallback when `cdt_piece`'s kernel refuses (a3's refusals: hull edges refuse/do not help 55,
  single-tet edge 54, link polygon 39, volume 16, no candidate 8 - boundary recovery was built for
  a convex region). It meshes 105 more pieces on a3 and makes 4 overlapping tets there (`[V1]` and
  `[V3]` FAIL), leaving the `[V9]` corners of a6a/a6b untouched. Reverted; `[JCT-CDT]` (print-only,
  under `RUSTMSPT_JCT_DIAG`) kept. The corner cell (a3 53265) still takes the whole-cell fan, and
  of its 16 tets the four touching the corner are all background.
  **The a3 corners, closed 2026-09-27.** Probing which site interned the corner showed two crease
  hubs, one per face around the checkerboard diagonal 20322–20489: a3's cube corner (0.8217,
  0.6783, 0.3217) lies 2.4e-8 off that diagonal (float32 input on the ...17/...83 convention), and
  `segment_pierces_triangle`'s 1e-9 barycentric slack admits it for both faces. Two coincident
  nodes (a3's 17 `[V2]` duplicates were all this), a zero-area fan triangle, an unmeshable piece, a
  whole-cell fan, no cube element at the corner. Measured and rejected on the way: a distance rule
  at `eps` (a3 −0.98, a8 −0.39) and at the ordering quantum (too fine - duplicates stay); genuine
  near-edge hubs occupy every decade from 1e-3 to 1e-7, so no distance separates them. The rule
  that works is the duplication itself (`pierces_shared_by_an_edge`): a pierce two faces sharing an
  edge both report within `[V2]`'s duplicate bound (`DUPLICATE_NODE_FRAC`, 1e-6 of the diagonal) is
  dropped for both. Default a3 `[V2]` and `[V9]` FAIL → **PASS**, duplicates 17 → 0, on-surface
  94.96 → 94.90; every other case and the gated path unchanged; R-P2 byte-identical. Left: a6a/a6b's
  `[V9]` node is a whole-cell fan centroid landing on the limb edge in a cell whose limb chords lie
  along walk edges - the fan M-2.3 removes.
  **Gated `[V2]` closed, 2026-09-27.** The gated path's duplicates (a8 11, a6b 1) were fragment
  vertices off every face plane but coincident with an existing cell node (a6b's limb corner, 1.7e-7
  from trace node 52349), interned anew because only on-plane vertices were matched. Now any arena
  node within `DUPLICATE_NODE_FRAC` is adopted: gated `[V2]` PASS on all nine, on-surface a8 98.445
  → 98.448, a6b unchanged, R-P2 byte-identical.
  **Gated `[V9]` on a8, located; one remedy refuted, 2026-09-27.** Gated a8's 50 `[V9]` nodes (default:
  0) sit on strut edges in cells §7.4 declined, which the gated path cones whole (`PlcPlan::Fan`,
  plc_path 2) and labels tet by tet; the strut's material there is a sliver ~1 `eps` thick (the
  strut face lies 1.75e-4 from lattice plane x = 0.296875; 0.5–1.2 % of the cell), so no fan tet's
  centroid is inside it. The default path tries §7.6's split before its whole-cell fan; the gated
  path does not. Calling `split_escalated_cell` on the gated boundary before `PlcPlan::Fan` was
  built and measured: a3 on-surface 97.11 → 97.37, but a6a **`[V3]` 28 boundary leaks, `[V6]`,
  `[V9]` open fans and `[V12]` side_elems FAIL** - the split reads the default path's face states
  (`cut_index`/`on_cut`), and the augmented boundary's trace points are unknown to it, so its
  pieces do not match the neighbours. Reverted. What would work is the split driven by the trace
  points the boundary carries - i.e. §7.4's own facet-split fan made able to separate an
  eps-thin sliver - not the default path's split.
  **The band was too generous: gated a6a `[V9]` closed, 2026-09-27.** Dumping a8's cell 507267
  (`RUSTMSPT_PLC_CELL`) showed a facet refused as "interior not covered" at `covered/want` 1.416 -
  over-covered: the band `facet_plane` gives the facet's own snapped vertices also admitted a
  sliver's apex, so both faces of a sliver lying along the facet counted. `face_on_facet_plane`
  applies the band to facet vertices only and `tol` to every other node. Gated a6a `[V9]` FAIL →
  **PASS** (on-surface 99.924 → 99.922), a3 97.11 → 97.15, a8 98.448 → 98.455 (its `[V9]` nodes 57
  → 61), a1/a6b unchanged; R-P2 byte-identical.
  **Buried faces out of S8, 2026-09-27.** Sizing M-2.2's class on a8 (`rim_worst` in the CSV)
  showed it bimodal - 728 of 850 declined cells have their rim point ≥ 0.1 edge from the face trace,
  so trace and fragment disagree about topology, not tolerance. a8 is the only self-intersecting
  case and S8 read the classifier's full triangle list, buried faces included, which S7 already
  drops. `PointClassifier::restrict_to_active` after S6 removes them from S8's traces and clips:
  gated a8 on-surface 98.455 → **98.796**, tets −5.8 %, link-polygon refusals 733 → 263, thin-tet
  438 → 171; default path and every other case byte-identical; R-P2 byte-identical on a8. The
  hull classes (863, 494) and a8's 61 `[V9]` nodes are unmoved, so buried faces were not their
  cause - they remain M-2.2's to explain.
  **M-2.2's archetype is a tessellation diagonal, not a missing trace point; the remedy measured
  mixed and is not kept (patch in the session scratchpad as `facet_merge.patch`).** Dumping a8 cell
  507276 (`RUSTMSPT_PLC_CELL`, facet `[4, 7, 3, 9, 1]`): the strut face x = 0.2967 is one plane
  tessellated into two input triangles; the fragment clip works per triangle, so their shared
  diagonal becomes a facet edge (9–1), and 9 - where the diagonal crosses the lattice face, exactly
  on the face trace's straight chord 3–6 (1.5e-14) - is a hull node the frozen boundary does not
  have. Merging coplanar same-component facets that share an edge, then dropping non-boundary
  vertices left collinear on the merged rim: a8 declines 3,457 → 2,871 (facet-edge class 1,451 →
  801) and elements −0.5 %, but on-surface 98.796 → 98.783 and `[V9]` 61 → 67 (the hull class rose
  863 → 940); a3 +0.04, a6a flat. Not strictly better, so reverted. The finding stands: the fragment
  side must see one facet per input *plane*, not per input triangle, and the next attempt should
  explain why the merged cells that still decline moved to the hull class before changing the rule.
  **`[V9]` PASS on all nine, both paths, 2026-09-28.** The last gated `[V9]` nodes (a3 7, a6b 2, a8
  61) were nodes within `eps` of a locked curve but not on it - a3's 45610 is a facet-split-fan node
  0.28 `eps` outside the cube, beside a sub-envelope sliver - which `curve_mesh_edges` declared as
  carrying the curve because it tested at `eps`. It now declares only edges on the curve within
  `DUPLICATE_NODE_FRAC`. The check is unchanged; the claim it tests became true. Meshes byte-
  identical (`data/output/acceptance_m28`), carried curves a3 90 → 94, a8 392 → 376; the default
  path's a6a/a6b/a7a `[V9]` pass too. **M-3.1's FAIL rule now holds**: both paths fail exactly
  `[V6]` on a3 and `[V13]`, and the gated path's `[V13]` is better on every case.
  **Link-polygon class, two more remedies refuted, 2026-09-28.** After the S2 fix the dominant
  stranded class on every case is "the link polygon has no valid triangulation". (1) Following the
  2-3-then-removal composite for up to four flips: inert on a3/a6a (counts identical). (2) Re-coning
  the hull edge's whole fan from a link vertex with the quad's other diagonal as the new hull: inert
  - a3 cell 3303's fan region is star-shaped from no link vertex, and the ones on the same cell face
  are coplanar with other hull faces there. Neither kept.
  **Flat tets double-covering a facet, 2026-09-28.** Reference case 1's largest refusal ("interior
  not covered", 45.6 % of its stranded area) dumped (`RUSTMSPT_PLC_CELL=1154`): facet [23, 8, 7, 26]
  read `covered/want` 2.000 exactly, because M-2.1's Steiner midpoint 26 lies on edge 7–23 that the
  Delaunay kept whole, leaving tets of volume 1e-22 and 8e-24 whose faces cover the facet twice.
  `remove_flat_quad_tets` splits every tet around such an edge at the point (and 2-2-flips a flat
  quad's pyramid to the other diagonal), guarded by hull, orientation and volume. Reference case 1
  interior class 124 → 84, on-surface 93.67 → **94.94** (default 95.25); gated a1 94.80 → **95.91**
  with `[V5]` WARN → PASS, a3 98.52 → **98.72**; the rest unchanged; R-P2 held.
  **Cavity re-cone for flat tets, 2026-09-28 (landed, marginal).** When neither the edge split nor
  the pyramid flip applies (reference case 1 cell 1730: a flat quad whose four face-neighbours have
  four different apexes), the flat tet and its face-neighbours are re-coned from one of their
  vertices, cavity boundary kept, guarded by the star test, no flat tet and volume. Reference case 1
  84 → 82 refusals, 94.94 → 94.96 %; the nine cases unchanged on-surface (a4/a8 a few dozen fewer
  tets); R-P2 held (a1, a4). Measured residue: on reference case 1, 379 of the 416 refused facets
  still have flat tets present at the facet check (`RUSTMSPT_FACET_DIAG` → `flat N`) - the next
  step, since removing them is what the "interior not covered" class now needs.
  **Baseline re-established on a new machine, 2026-09-28** (`data/output/acceptance_base`, build of
  `111b674`): the nine cases on both paths reproduce the previous session's on-surface shares to the
  last printed digit and the same FAIL sets; the three reference cases likewise (gated 94.956 /
  97.121 / 99.577 %, default 95.248 / 96.097 / 96.831 %, gated tets 0.96× / 0.74× / 1.34×);
  `cargo test --release` 677 passed. The private handoff note is retired: this document is the only
  progress record from here on.
  **Flat tets removed by edge removal, 2026-09-28 (landed).** `RUSTMSPT_FLAT_DIAG` (print-only)
  classified the flat tets `remove_flat_quad_tets` skipped on reference case 1: interior ones with
  four distinct neighbour apexes, and ones lying on a cell-face quad whose interior neighbours take
  the other diagonal with two different apexes. A fourth move removes one of the flat tet's edges
  that is neither a hull nor a facet edge (`remove_edge`), kept only when the cell's flat count
  strictly falls; the caller accepts the pass by flat count instead of tet count (a ring split or an
  edge removal can add tets, and the old `len() <` guard discarded them). Reference case 1 refused
  facets 406 → 226, gated on-surface 94.956 → **95.346 %** (default 95.248: **M-3.1's case-1 `[V13]`
  gap is closed**), case 2 97.121 → 97.439, case 3 99.577 → 99.618; gated a1 95.911 → **96.882 %**
  (no clusters ≥ 10 % left; visually four yellow patches on the sphere gone), the other eight cases
  equal, the default path unchanged everywhere, FAIL sets unchanged, R-P2 byte-identical (a1, a3),
  677 tests pass (`data/output/acceptance_m21e`). Residue: the cell-face class - removing the
  interior diagonal finds no valid link triangulation or makes another flat tet, because other
  boundary points of the same cell face are coplanar with the ring.
  **The zero-area face triangle and the interior Steiner point; M-3.1 met, 2026-09-28 (landed).**
  Reference case 3's last gated `[V9]` node (208186, face x = 0) sat between two cells declined as
  thin. Their flat-tet removal *succeeded* and was rejected for moving the hull: the flat tet's hull
  face was itself a triangle of area 2e-17 × longest² in the frozen boundary - three collinear trace
  points inside the face, joined by Bowyer-Watson (only the rim case was split, `rim_of`).
  `constrained_face_triangulation` now splits such a triangle with its neighbour across its long edge
  (height < 1e-9 of the longest edge, its segment split's own bound); a function of the face, so J1
  holds. With it, `constrained_tets_with_steiner` gains move (c): on a thin-tet or uncovered-facet
  refusal, each flat tet with a hull face and all four vertices on one cell face (co-circular face
  nodes) gets a point strictly inside the cell, a quarter of its longest edge in from its centre,
  and the Delaunay is re-run; never on a cell face, rolled back on refusal. Gated on-surface:
  reference cases 95.346 → **95.733**, 97.439 → **97.794**, 99.618 → **99.692 %**; a3 98.723 → 98.905,
  a4 99.482 → 99.585, a6a 99.953 → 99.983, a6b 99.904 → 99.922, a8 98.897 → **99.124 %**; a1, a2,
  a7a, a7b unchanged; elements +0.1–0.9 %; reference case 3's FAIL set {V9, V13} → **{V13}**; every
  other status unchanged. Ablation (`data/output/abl_{both,noC}`): the 2D split alone removes the
  `[V9]` node and changes nothing else; every on-surface gain is move (c)'s. Default path byte-
  identical on all nine and unchanged on the reference cases; R-P2 byte-identical (a1, a3, a8);
  677 tests pass; gated time on this host a3 20.2 → 17.4 s, a4 77.4 → 73.1 s, a8 206.6 → 194.2 s
  (`data/output/acceptance_base` is this host's budget line - M-1.3's was recorded on the old one).
  **M-3.1 now holds on all twelve cases**: the gated FAIL set equals the default's everywhere and its
  `[V13]` is better on every case where the default is below 1.0.
  **Move (d), interior Steiner points above hull faces recovery cannot flip — measured 2026-09-28, landed on
  owner decision D-11 (a) the same day: geometry first, quality is M-5's.** Ranked by stranded area (`[PLC] P3 stranded by refusal`), "the
  link polygon has no valid triangulation" is the largest class on every case (49–97 %). Extending
  move (c) to that refusal and to "neither the boundary nor the facets survive" - a point strictly
  inside the cell a quarter of the longest edge in from each current hull face not in the frozen
  boundary, so the flip becomes a pyramid's - removes the class from the ranking on a1, a3 and a6b.
  Gated, against `d7a8245` (`data/output/acceptance_m2d` vs `data/output/abl_both`): on-surface a1
  96.882 → 97.799, a3 98.905 → **99.638**, a4 99.585 → 99.774, a6a 99.983 → 99.995, a6b 99.922 →
  99.996, a7a 99.772 → 99.962, a7b 99.778 → 99.968, a8 99.124 → **99.779 %**, a2 100; reference
  cases 95.733 / 97.794 / 99.692 → **97.697 / 98.944 / 99.795 %**; every status unchanged; default
  path byte-identical; R-P2 byte-identical (a3, a8). **But it costs P2 and P4:** tets +1.0–8.1 % on seven cases and **+27–29 % on
  a7a/a7b**, and tets below `[V4]`'s dihedral floor rise in count and share (a8 55,233 → 78,490,
  4.1 → 5.4 %; a3 25,816 → 29,413; a6a 20,299 → 23,329). Not strictly better, so it was held for D-11; the owner chose (a) - optimising AR is the later
  quality phase's job, now the geometry must be right - and it is landed. The cost is M-5's to
  recover: find why a7a/a7b's plates take +28 % (likely one point per
  wrong hull face where one per quad would do, or points inserted in cells whose refusal then stays
  unchanged), and let the point sit where it does not make slivers.
  **Measure the fallback first, again: a kernel apex for a non-star piece, 2026-09-29.** Charging
  the whole-cell fans' off-surface area to the facet-split fan's OWN refusal (`RUSTMSPT_SPLIT_DIAG`
  folds it into the census reason) put one reason on top everywhere: "a piece is not star-shaped
  from its centre" - all of a1's stranded area, ~75 % of a8's, 22 % of a3's. The centroid is one
  apex; any point of the piece's kernel fans it validly, and for a point off the boundary "every face
  takes one exact orientation sign" is exactly the kernel test, so the guard is unchanged and only the
  apex moves: the centroid, then the midpoints to each vertex, then a 5x5x5 grid over the box, first
  that passes. Gated (`data/output/acceptance_m21h`): a1 97.799 -> **100.000** and a7b 99.968 ->
  **100.000**, both `[V13]` PASS; a8 99.833 -> 99.942, a4 99.911 -> 99.971, a3 99.644 -> 99.716,
  a7a 99.962 -> 99.994, a6b 99.996 -> 99.999, a6a unchanged; reference cases 97.697 -> **99.597**,
  98.956 -> **99.789**, 99.854 -> 99.939; `[V5]` WARN -> PASS on a4 and a7b; no status worse; elements
  +0.0-0.8 %; default path unchanged; R-P2 byte-identical on a4/a8; 677 tests. Whole-cell fans: a1/a2/
  a7a/a7b **0**, a3 110 -> 106, a4 19 -> 11, a6b 4 -> 2, a6a 5, a8 738 -> 630 - M-2.3's remaining
  population. Refuted on the way (a parse error, recorded so it is not repeated): splitting facet
  edges at collinear nodes is inert - the "node exactly on the missing edge" reading came from a
  census column that had shifted, and the real nearest node is 0.05+ of an edge away.
  **One piece where no cap is left (D-12, 2026-09-29).** The facet-split fan's largest remaining
  refusal on a8 was "no surface separates the cell into two pieces" (534 cells), almost all cells
  whose surface lies in their own faces (a8's axis-aligned struts on lattice planes): nothing to
  cut, one material, yet the refusal sent them to the whole-cell fan. `facet_split_fan` now takes
  such a cell as one piece - only when no cap triangle is left at all, since a cap that is inside
  and still separates nothing is a surface passing through. Gated (`data/output/acceptance_m21j`):
  whole-cell fans a8 630 -> **96**, a3 106 -> **23**, a6a 5 -> 4, a6b 2 -> 1; tets a8 -4,107, a3
  -754; P3 and every status unchanged. Cost: a8 one more misattributed cell (5.06e-11), traced to
  cell 738494 - its facet's rim was matched onto face nodes, leaving a sliver of component 1 thinner
  than `eps` (1e-4, ~2 % of the cell's edge) that one label cannot see and the old per-tet fan
  caught by one centroid. Not strictly better, so it went to the owner as D-12: (a) land it.
  **An on-surface triangle goes where it closes a piece (D-13, 2026-09-29).** The fallback's next
  refusal was "a piece's surface does not close - the open edge is one the CELL's triangulation
  carries" (a8 61, a4 11 - all of a4's -, a3 6). Dumped on a4 cell 287266: facet [6, 5, 4, 7] is
  bent over a cell edge, its in-face part coincides with boundary triangle [4, 5, 6], and that
  triangle - every corner on the surface - was placed by its own centroid, which is also on the
  surface, so it landed below: the piece above was open along 4-5 and 5-6 and the piece below
  carried it as an appendix. `facet_split_fan` records triangles placed that way and moves one to
  another piece when the move leaves BOTH closed (a partial-improvement rule measured the same).
  Gated (`data/output/acceptance_m23b`): a4 whole-cell fans 11 -> **0**, on-surface 99.971 ->
  **100.000**; a3 99.716 -> **99.850**; a8 fans 96 -> 39; reference cases 99.597 / 99.789 / 99.939 -> 99.641 / 99.851 / 99.942 %; tets within
  +0.01 %; default path unchanged, R-P2 byte-identical, 677 tests. Cost: misattributed
  micro-slivers a4 0 -> 4 (2.2e-12) and a8 3 -> 56 (+1.1e-10), every one a tet of 1e-13-1e-11 whose
  centroid is ~1e-5 inside the body, i.e. inside `eps`; a4 `[V5]` PASS -> WARN. Owner decision D-13
  (a), with the standing rule recorded in §12.
  **Cap facets fanned from the least-thin apex, 2026-09-29.** Next refusal: "the open edge is one
  only the CAP carries" (a3 11, a8 3, a6a 2, a6b 1). a3 cell 17931: component 1's facet
  [19, 25, 12, 5] carries 25 within rounding of the line 19-12 - all three are on both surfaces,
  i.e. on the intersection line - and the cap's fan from the first vertex made the sliver
  [19, 25, 12], which lies along that line, has no side of component 0 and was separated from
  [19, 12, 5], leaving the piece open along 12-19. A zero-area test does not see it (it is thin, not
  flat); fanning each facet from the apex that maximises its thinnest triangle does. Gated
  (`data/output/acceptance_m23d`): a3 99.850 -> **99.896**, `[V6]` adjacency 47 -> **23**, whole-cell
  fans 21 -> 12, misattributed 7 -> 4; a8 99.942 -> 99.945, fans 39 -> 33, misattributed 56 -> 26;
  a4 misattributed 4 -> 5 (a sliver 6e-5 from the cube face, inside `eps`; landed under D-13's
  standing rule); reference cases 99.641 / 99.851 / 99.942 -> 99.814 / 99.893 / 99.950 %; tets within
  +-0.08 %; statuses, default path and R-P2 unchanged; 677 tests.
  **Several shells, several pieces, 2026-09-29.** "The soup is closed but in more than one shell"
  (a8 6): one side of a cut can be two disjoint closed solids. Each shell that closes on its own is
  now its own piece and region (nested shells still fail the partition check). Gated
  (`data/output/acceptance_m23f`): a8 whole-cell fans 33 -> 27, on-surface 99.945 -> **99.956**;
  reference case 3 99.950 -> 99.953; every other case and metric identical; R-P2 held, 677 tests.
  Tried and reverted on the way (inert): treating
  a node `conform_cap_rim` inserted into a cap as on the surface - a8 cell 29842 then failed on its
  cap's overlapping coplanar facets instead (one component's overlapping struts), same counts.
  **The kernel's own vertices as the last apex, 2026-09-29.** The fixed candidate list missed thin
  kernels (a8 10 non-star refusals). `kernel_vertex_mean` enumerates the intersections of three face
  planes on the inner side of every face and takes their mean - interior whenever the kernel has an
  interior - still accepted only by the exact test. It fans all ten: a8 whole-cell fans 27 -> **17**,
  on-surface 99.956 -> **99.971**; reference cases 99.814 / 99.893 / 99.953 -> 99.910 / 99.919 /
  99.957 %; every other case identical, R-P2 held, 677 tests (`data/output/acceptance_m23g`).
  **Two solids pinched along an edge, 2026-09-29.** a3's "edge carried three times or more" (4
  cells): cell 80037's cube has its sharp edge on a cell edge (S7 snap), so the outside wraps round
  it in two sectors touching only along that edge - one soup with a four-fold edge. Joining
  triangles only across edges carried exactly twice (`manifold_components`) separates them; each
  closes. a3 whole-cell fans 12 -> **8**, on-surface 99.896 -> **99.917**; reference case 3 99.957 ->
  99.958; every other case identical, R-P2 held, 677 tests
  (`data/output/acceptance_m23i`). Tried and reverted (inert): dropping exact duplicate cap
  triangles within a component (a8's overlapping struts).
  **Flaps shed, 2026-09-29.** a3 cell 97751 ("open edge the CELL carries"): component 1's cap
  triangle [20, 14, 5] sat in a piece it does not bound, open along 5-14 and 5-20, because every
  boundary triangle around node 5 is in the other piece. A piece that does not close now sheds
  triangles another piece also carries, greedily while its open edges fall, kept only if it closes.
  a3 whole-cell fans 8 -> **4**, on-surface 99.917 -> **99.927**, `[V6]` adjacency 23 -> **12**; every
  other case identical; reference cases 99.910 / 99.919 -> 99.936 / 99.935 %; R-P2 held, 677 tests
  (`data/output/acceptance_m23j`).
  **Refuted: dropping a facet the rim matching left without area** (a6a/a6b "a facet has no
  area", 2026-09-29). It lets a6b's two cells mesh (whole-cell fans 2 -> 0, on-surface -> 100.000)
  but turns `[V9]` PASS -> FAIL on a6a and a6b: the collapsed facet lies along the limb's rim curve,
  and it is what labels the curve nodes. Reverted; the facet has to stay as a constraint on the
  curve, not be skipped.
  **Corners exactly on the surface have no side, and a single-plane cap is its outline,
  2026-09-30.** a8's 13 "a boundary triangle straddles the surface with no node on it" were one
  mechanism, found by printing each boundary node's side beside what the cut recorded for it
  (`[SIDE-NODE]`, print-only; `RUSTMSPT_PLC_CELL` now takes a list): every straddling triangle has
  one corner EXACTLY on the surface that the fan's side oracle sent to the classifier - a coin toss.
  Two kinds: nodes the cut already put on the component's surface (S7 snaps, K2 promotions - cells
  6619, 72050, 78462, 841020), now read from the same record M-2.2 uses; and boundary nodes
  `conform_cap_rim` split the cap's rim at (cell 29842's node 5), now sideless for that cap. Cell
  29842 then showed its cap covering part of the cross-section twice: the component's three
  facets all lie in the strut face's plane and snapping had left one a bow-tie ([8, 6, 1, 7, 4],
  edges 8-6 and 1-7 crossing) - the "overlapping coplanar facets" of the pause note, which is why
  the contained-facet rule was inert. A closed surface cannot end in the cell, so their union is the
  plane's convex cross-section: `coplanar_cap_outline` caps it by the facet vertices' hull, only
  when every vertex is on that hull and no other component's facet shares a vertex (without the
  second condition a3's cube face lost its sphere curve and a3 went 4 -> 54 whole-cell fans; a
  star-apex fan and an area-sum trigger were tried first and are refuted - the areas tile exactly).
  Gated (`data/output/acceptance_m24c`): a8 whole-cell fans 17 -> **4**, on-surface 99.971 ->
  **99.993**; a6a 4 -> **3**, 99.995 -> 99.998; reference cases 99.936 / 99.935 / 99.958 ->
  **99.946 / 99.938 / 99.963 %**; every other case's P3 and every check status unchanged; elements
  within +0.19 % (a3 +506, landed under D-11); default path unchanged; R-P2 and tests below.
  Ablations on a8: without the two sideless rules, or without the outline, 5 whole-cell fans each.
  **Three more classes closed, 2026-09-30.** (a) a3's four "open edge the CELL carries" (cells
  17257, 41505, 75954, 100254, one configuration at four corners): both components' caps share a
  small triangle where the intersection curve meets the cube edge, and the second cut left it with
  a boundary triangle as a fin that D-13 would not move because the source piece closed only after
  the flap was shed. D-13 now keeps a move that closes the target and lowers the source's open
  edges; flap shedding finishes the source. (b) a6a's two "open edge only the CAP carries" were
  "pieces" made only of cap triangles both components share on the contact plane; such a piece
  bounds no volume and is dropped. (c) a6a/a6b's "a facet has no area": a facet the rim matching
  folds onto a line is skipped and **the nodes it interned are rolled back** - the refuted skip of
  2026-09-29 kept one (a6b node 51389 on the limb's curve 14, N_ID [0]), which is all `[V9]` failed
  on. Gated (`data/output/acceptance_m24f`): whole-cell fans a3 4 -> **0**, a6a 3 -> **0**, a6b 2 ->
  **0**; a3 on-surface 99.927 -> **99.991**, `[V6]` FAIL -> **PASS** (adjacency 12 -> 0); a6a 99.998
  -> **100.000**, cover 99.668 -> 99.872, `[V5]` WARN -> PASS; a6b 99.999 -> 100.000; elements a3
  +180, a6a +16, a6b -9; every other case identical; reference cases 99.946 / 99.938 / 99.963 ->
  **99.970 / 99.963** / 99.963 %; default path unchanged, R-P2 byte-identical, 677 tests. Tried and
  reverted (inert): dropping cap
  triangles an earlier component's cap already used (a3, before (a)); the contained-facet filter
  again (a8, 4 -> 4); the outline rule without its "every vertex on the hull" condition (a8, 4 -> 4);
  a side oracle treating a node inside or on any facet as on-surface (a8 cell 835403 moved from
  "straddles" to "open edge only the CAP carries", count unchanged).
  **A strut face on a cell face, 2026-09-30.** a8 cell 425581: the strut's face coincides with
  part of the cell face x = 0.1713, its cap triangles there are dropped as lying in the face, and
  node 9 - on the dropped facet's edge and on the cell edge - was neither a facet vertex nor on a cap,
  so the classifier tossed a coin; the coincident region's face triangles, all placed by centroid,
  then landed together in the wrong piece. The side oracle now gives no side to a node on or inside
  any facet of the component (`facet_within`), and D-13 offers each edge-connected patch of
  centroid-placed triangles whole. Each alone was inert on a8 (4 -> 4); together (gated,
  `data/output/acceptance_m24g`): a8 whole-cell fans 4 -> **3**, 99.993 -> 99.995; reference cases
  2 / 3 99.963 / 99.963 -> 99.966 / **99.995 %**; elements +3 to +56; other cases identical;
  default path unchanged, R-P2 byte-identical, 677 tests.
  **The edge lying IN the facet: M-2.1 reaches zero, 2026-09-30.** a8's last three cells looked
  like overlapping coplanar facets of one self-overlapping component, and S2's coplanar overlay was
  suspected; printing the raw clipped facets before adoption (`[RAW-FACET]`) showed they tile
  exactly. The overlap was made by M-2.2's adoption: where a strut face lies on a lattice plane,
  the cell edge lies IN the surface, the on-surface list names only its endpoints, and a rim vertex
  at t = 0.614 of edge 1-2 (cell 29843) was moved to the endpoint at t = 1, folding its facet over
  the neighbour (403695 likewise; 835403's "crease inside a face" was the same fold seen from the
  face). A node on the edge now also qualifies when it lies in the facet's own plane. Gated
  (`data/output/acceptance_m24h`): a8 whole-cell fans 3 -> **0**, on-surface 99.995 ->
  **100.000** (worst corner 35.6 % -> 1.9 % of an element); elements a3 -25, a8 +4; every other
  case identical; reference cases 99.970 / 99.966 / 99.995 % unchanged; default path unchanged, R-P2
  byte-identical, 677 tests. **Whole-cell fans are now 0 on all nine acceptance cases** - but NOT on
  the three reference cases, which the matrix includes: deleting the arm (the M-2.3 patch, kept at
  `data/output/patches/m23_delete_whole_cell_fan.patch`, 677 tests green, the nine cases
  byte-identical to `acceptance_m24h`) made all three reference cases refuse
  (`data/output/acceptance_m25a/ref_gated.txt`: "a facet's edges are all there but its interior is
  not covered", "a facet edge is not an edge of the tetrahedralisation", "a boundary node is off the
  hull, and the facets fail too"). M-2.3 waits for M-2.1 on the reference cases. Census at HEAD
  (`data/output/acceptance_m25b`, `RUSTMSPT_SPLIT_DIAG`): whole-cell fans reference case 1 **4**,
  2 **11**, 3 **16**; by the facet-split fan's refusal, "open edge only the CAP carries" 12, "open
  edge the CELL's triangulation carries" 10, "not star-shaped from its centre" 6, "straddles" 1.
  **Refuted on the reference cases, 2026-09-30:** (1) dropping cap triangles an earlier
  component's cap already used - reference case 1 4 -> 5; (2) §7.6's "a partly coincident surface
  does not cut the piece again" (`any`) transplanted into the facet-split fan - it cut whole-cell
  fans (ref 1 4 -> 3, ref 2 11 -> 5) and is **worse**: a3 on-surface 99.991 -> 99.930 with `[V6]`
  PASS -> FAIL (adjacency 0 -> 44) and 136 misattributed cells, a6a `[V9]` FAIL, reference cases
  1 / 2 99.970 / 99.966 -> 99.929 / 99.932 (`data/output/acceptance_m25c`). It skips cuts the piece
  needs; fewer whole-cell fans bought by leaving pieces unseparated is not progress.
  **Landed on the reference cases, 2026-09-30:** (a) on a cell edge the edge decides first - a
  `weld` match stands for an edge vertex only when the matched node is on that edge (ref 1 cell
  17275: a node on one face plane 5.7e-5 away had beaten the edge's crossing 1e-4 away); (b) flap
  shedding reads "another piece carries it" from before any shedding (ref 1 cell 27812: a fin both
  halves received). Gated (`data/output/acceptance_m25d`): reference case 1 99.970 -> **99.993**,
  3 99.995 -> **100.000**, 2 identical; elements +45 / -44; the nine acceptance cases
  byte-identical; default path unchanged; R-P2 byte-identical; 677 tests. Reference-case
  whole-cell fans 4 / 11 / 16 -> **2 / 11 / 9**. `[ADOPT]` (print-only) added.
  Then D-13's patch move joins two on-surface triangles only across a shared edge **in one plane**
  (reference case 3 cell 84796: the face-coincident patch had been merged through a corner with
  four triangles of other faces): reference case 3 9 -> **5**, elements +15, everything else
  identical, R-P2 byte-identical, 677 tests (`data/output/acceptance_m25e`). Refuted again on top of it: dropping cap triangles an
  earlier component's cap used (ref 1 / 2: 2 / 11 -> 5 / 20). **Standing: 2 / 11 / 5 = 18
  whole-cell fans, all on the reference cases**; reference case 2's eleven are two components whose
  facets partly coincide (cell 44176: [14, 24, 0], [15, 24, 14], [25, 15, 16] and more under both).
  Three more variants aimed at that class were measured on reference cases 1 / 2 (baseline 2 / 11)
  and refuted, 2026-09-30: not re-adding a cap triangle the piece already carries (5 / 20 - the
  same as the global dedup, so where the copies go is not the defect); that plus deciding a
  coincident triangle's half by stepping off it to the piece's known side of the earlier cut
  (8 / 24); the side decision alone (2 / 11, inert - so the coin toss on coincident triangles is
  not what fails these cells either). The next step is a geometric reading of one cell's pieces
  (44176 splits into four, the open one a sliver between two nearly coincident walls), not a sixth
  rule. **Read, 2026-09-30:** the five coincident triangles sat in THREE pieces - the two they bound
  and one where they are a sheet (open exactly along the sheet's rim), while one boundary triangle
  that piece needed had gone to another piece. Flap shedding now also sheds an edge-connected run
  whole and keeps a shedding that lowers open edges, and D-13 moves and shedding alternate for
  three rounds. Reference case 2 whole-cell fans 11 -> **10**, 99.966 -> 99.968, elements +58,
  everything else identical, R-P2 byte-identical, 677 tests (`data/output/acceptance_m25f`).
  **Standing: 2 / 10 / 5 = 17.** The "not star-shaped" class (7 of the 17) is non-convex pieces the
  constrained kernel also refuses ("a boundary node is not on the hull", "the link polygon has no
  valid triangulation"). **Refuted, 2026-09-30: greedy ear peeling from the piece's own nodes.** A
  lenient version took reference case 2 10 -> 9 and passed the matrix
  (`data/output/acceptance_m25g`: 99.968 -> 99.973, nine cases byte-identical) - but its ear test
  cannot see a new face overlapping a coplanar boundary face, and on a plain triangulated cube it
  peels ten tets and leaves a whole shell, so that gain came from the final volume check catching
  everything else. With complete coplanar-overlap checks it stops on a U prism and reference case 2
  goes back to 10; with a remnant fanned from its kernel it still stops (a twisted-prism remnant is
  reached first, then a non-star one). Not landed; the lenient code and its tests are kept at
  `data/output/patches/m25_peel_ears_lenient_with_tests.rs.txt`. This class needs a real
  tetrahedraliser for a non-convex piece - the hull tetrahedralised with the piece boundary as
  constraints and the outside carved off, or Steiner points - which is a capability, not a rule.
  **Where M-2.1/M-2.3 stand at the end of 2026-09-29 (session pause).** Whole-cell fans, the arm
  M-2.3 deletes: **0 on a1, a2, a4, a7a, a7b**; a3 **4**, a6a **4**, a6b **2**, a8 **17** (from a3 110,
  a4 19, a6a 5, a6b 4, a8 738 this morning). Every change landed today was strictly better or taken
  under D-12/D-13; `[V1]`/`[V3]`/`[V9]` clean throughout, R-P2 byte-identical, 677 tests. Remaining
  classes, by the facet-split fan's own refusal (`RUSTMSPT_SPLIT_DIAG`, `[SPLIT-FAIL]`):
  a8 13 "a boundary triangle straddles the surface with no node on it", all one mechanism not yet
  found - cell 29842 has three coplanar overlapping facets of one component ([7, 1, 6, 5, 0],
  [1, 6, 8] inside [7, 4, 8, 6, 1], which shares [7, 1, 6] with the first), but leaving contained
  facets out of the cap and keeping duplicate cap triangles once measured **exactly inert** alone
  AND together (patch `data/output/patches/m23_overlap_caps.patch`), so the straddle is not that
  overlap; next step is to print the classifier's side for node 5 against each of the three
  facets' planes, not to try another cap rule. a8 3 and a3 4 "the open edge is one the CELL's
  triangulation carries" (a3 3 of them "neither the boundary nor the facets survive" at the
  kernel); a6a/a6b 4 "a facet has no area" - the collapsed facet lies on the limb's rim curve and
  must stay (skipping it fails `[V9]`); a6a 2 and a8 1 "open edge only the CAP carries".
- **M-2.2. Boundary consistency: a facet vertex on the cell's boundary is a trace point.** The
  "hull carries a node the boundary has never heard of" class (a8 1,200 cells, 38.3 % of its
  stranded area) is the residue of record §6.43, which took it from 72 % of the population by making
  the facet's rim come from the faces. What is left is a facet vertex that lies on a cell face
  (within the face's own plane tolerance, `6e-15` of an edge on a3) and is not in that face's
  triangulation. Two consistent resolutions, decided by measurement: intern it as a trace point of
  that face (both owners then see it — J1), or weld it to the face's nearest trace point when within
  `q`. Never a Steiner point on the face. Analysed with record §6.42's method (per-cell facts to a
  CSV, `RUSTMSPT_PLC_CSV`) before either is built.
  *Acceptance:* the two hull classes fall to 0 on a8 with `[V3]` held; the census names any
  residue. *Tier T3. Multimodal: no.*
  **Landed 2026-09-29 - it was neither resolution: the vertex is on a cell EDGE and the edge had
  already decided.** `RUSTMSPT_HULL_DIAG` put every intruding vertex on two face planes at once, on a
  boundary edge, 0.03-0.5 of an edge from any boundary node (a4 all of them; a8 most, plus
  coplanar-facet cases). Dumping a4 cell 436445 (`RUSTMSPT_TRACE_PROBE`): the edge lies in the cube
  face on a lattice plane, S7 snapped endpoint 104408 onto that face 2.5e-4 past the cube's
  perpendicular face, and invariant K2 promoted the edge's crossing to it - so the faces and every
  cell around the edge carry the crossing at 104408, while the fragment clip puts it at the raw
  intersection 0.028 edge away and `weld` refused the match. Neither the plan's "intern as a trace
  point" (it would contradict K2's decision on the edge, in six cells) nor "weld within `q`" (the
  gap is ~250 `q`) was right. `plc_attempt` now gives a rim vertex on a cell edge the nearest
  boundary node on that edge the cut marks on-surface for its component - crossing nodes plus
  `on_cut` parents - with no distance bound, because it is K2's decision read, not a tolerance.
  Result (`data/output/acceptance_m22a`): both hull classes **0** on a3, a4 and a8 (a8's
  coplanar-facet vertices went with them); gated on-surface a4 99.774 -> **99.911**, a8 99.779 ->
  **99.833**, a3 99.638 -> 99.644, reference cases 97.697 / 98.944 -> 98.956 / 99.795 -> **99.854**,
  the other six cases unchanged to the digit; elements down slightly (a8 -818); every check status
  unchanged on all twelve; default path unchanged; R-P2 byte-identical on a4/a8; 677 tests. Residue
  now: a8 facet-edge 51 % / thin-tet 27 % / interior 18 %, a4 interior 75 %, a3 interior 69 % -
  all facet or thin classes, none hull.
- **M-2.3. Delete the whole-cell centroid fan (R1).** When M-2.1 reports 0 on the matrix, the arm is
  removed: a cell neither §7.4 nor the facet-split fan can mesh is `Err` naming the cell, with the
  per-cell dump `RUSTMSPT_PLC_DUMP` already writes. Not "kept for safety": a fallback that abandons
  conformity is the defect this plan exists to remove, and an error names a bug where a fan hides it.
  *Acceptance:* the arm's code is gone; `plc_path = 2` never occurs; the matrix is unchanged to the
  byte from M-2.1/M-2.2's production run. *Tier T2. Multimodal: no.*
- **M-2.4. Retire `contact_chamfered_by`.** It excuses a two-component step within the fan's
  chamfer of a coincident patch (record §6.66). With no fan there is no chamfer; the exact-contact
  declaration through coplanar facets and `declare_contact_components` is what remains. Measure its
  count on the matrix; when it is 0 everywhere, delete it and its tolerance.
  *Acceptance:* deleted, `[V6]` unchanged. *Tier T1. Multimodal: no.*
  **Measured 2026-09-28 — blocked by resolution, not by the fan.** `[G6-3] contact_chamfered_by
  declared N` (print-only, `RUSTMSPT_CUT_DIAG`): a6a 895 gated / 533 default, a6b 2,396 / 1,816; 0
  wherever S2 declares no coincident patch. On gated a6a 859 of the 895 lie between two
  §7.4-meshed tets (plc_path 1) and within ±6e-5 of the contact plane x = 0.5817 - not a fan
  chamfer but lattice faces S7 snapped into the plane that extend past the limb's footprint: the
  limb is 0.006 thick in z and a lattice face ~0.0069 across, so `declare_contact_components`'s
  "every corner on a coincident patch" cannot hold and this rule declares the rest. It retires when
  the contact strip is resolved by more than one face, not before.
- **M-2.5. P3 residual audit.** With the fan gone, charge every remaining off-surface face to
  `plc_path`, `escalation_reason` and node kind (record §6.9's decomposition) on all twelve cases.
  Expected residue: §6's table where the trace agrees with §5.2 (exact by construction), and the
  input's own tessellation (chording, reported as `chord_*`, not a violation). Anything else is a
  named defect with an owner.
  *Acceptance:* `[V13]` FAIL count on the matrix, and for every case still failing, the arm and the
  mechanism. *Tier T2. Multimodal: yes — the same cutaways as M-2.1.*

### M-3 — One path

- **M-3.1. The control.** M-1.1's harness on all twelve cases: the gated path against this
  document's default column. The rule is the record's own (`AGENTS.md`: *"land it as the default when
  it measures strictly better; keep the handle only when it measures worse"*): not worse on any FAIL
  gate on any case; `[V13]` better on every case where the default is below 1.0; elements within
  M-1.4; time within M-1.3. Today the gated path **fails** this on `[V9]` (a3, a6a, a6b, a8 — four
  cases the default passes) and on `[V2]` (a6b, a8), §2.2 — so M-3 waits for M-2 and M-1.6, both
  inside the phases that precede it (no cycle, MG-12).
  *Acceptance:* the control table, all rows green. *Tier T2. Multimodal: no.*
- **M-3.2. Delete the handles.** `RUSTMSPT_PLC_PASS` (the path becomes the path), `RUSTMSPT_CDT`
  (measured worse — the `subdivide_cell` route it enables is deleted with it, not parked),
  `RUSTMSPT_NO_JCT_CUT`, `RUSTMSPT_NO_K1_SECOND_CUT`, `RUSTMSPT_NO_MIXED_BY_CENTRE` (ablation
  switches — deleted; an ablation is a test, and the tests hold the ablation). Print-only and
  dump-only variables stay and are listed in the docs as such.
  *Acceptance:* `grep -rhoE 'RUSTMSPT_[A-Z0-9_]+'` over the mesher and its verifier (R3's scope)
  contains no behaviour-changing name; the matrix is byte-identical to the gated column before
  deletion. *Tier T1. Multimodal: no.*
- **M-3.3. Refactor `cut_lattice`.** 4,452 lines including the whole PLC pass. Split into stage
  functions (trace interning, face augmentation, per-cell PLC attempt, assembly, interface
  derivation, diagnostics) of at most ~500 lines each, with the PLC pass in its own file. No
  behaviour change.
  *Acceptance:* matrix byte-identical; `cut.rs` gains inline tests for the trace interning and the
  on-edge rule (the two places the record's worst defects lived, §6.35 and §6.65); `junction.rs`
  gains its own test file. *Tier T2. Multimodal: no.*

### M-4 — Conditioning, thin features, and P1

- **M-4.0a. One X per connected closed component (MG-04).** R-A2 gives every connected closed
  component its own X; the pipeline creates one `ArrangeComponent` per input file and decides
  `kind` from the whole file's closedness: one STL holding two disjoint cubes yields
  `ComponentX = [1]` (S0 logs 2 components); one holding a cube and a disjoint open quad yields a
  single **sheet** component, `ComponentClosed = 0`, and the cube — closed — claims no volume under
  the sheet rule. S0's connected components never become the semantic table. Fix: keep three
  identities apart — input file, original shell, repaired component — assign a stable X per S0
  connected component, inheriting the file's priority and material mapping; decide `kind` per
  component; emit the source map (file → shells → X) in `s00` and the contract; the verifier loads
  reference surfaces through that map instead of assuming `X − 1` indexes `surfaces`. Decide and
  record whether a shell nested inside another is a separate component or a solid with a cavity, so
  fixing this does not silently change that. *Acceptance:* two solids in one file ≡ the same two in
  two files (geometry and labels compared through the map); solid + sheet in one file keeps the
  solid's non-zero volume; priority and materials addressable per component; the two `reproduce.py`
  cases become fixtures A-14a/b (B.9). The existing `detects_multiple_components` test uses two
  input meshes and does not cover this. *Tier T3. Multimodal: no.*
- **M-4.0b. Closure is proved per component (MG-05).** `rebuild_topology` decides a component's
  closure from a global `edge_faces` incidence, so a face of *another* component covering this
  one's opening counts as its partner: a five-face cube declared `solid` plus an independent sheet
  over the missing face comes out `ComponentClosed = [1, 0]` at `s02` ("solid closed, 0 closure
  defects") though the solid's own faces still leave a hole — and it is then classified by parity.
  The member set is filtered by the representative `face.component`, not every tag of a merged
  face, and incidence > 2 with no incidence-1 edge is not treated as a manifold defect. Fix: per-X
  oriented edge incidence over the component's full member set (every tag of a merged face
  expanded); boundary, non-manifold and orientation defects checked separately; GWN routing decided
  from the component's own defects only. *Acceptance:* the `borrowed_closure` case keeps the solid
  defective (GWN, WARN); fully and partly coincident solids keep independent topology records;
  shared edges and vertices never change the other's closure; fixture A-15 (B.9). **Widened
  (geometry D-42):** the same primary-tag selection leaves a component whose every face was merged
  under another component's tag with *no* classification, which S6 defaults to `Sheet` — two
  identical cubes at equal priority (`a2_cube.stl` twice) come out of S2b as "1 solid, 0 sheet"
  and out of S6 as one solid plus one sheet with two region keys, the second cube's material
  gone. The per-X member set fixes both; *acceptance adds:* fixture A-16 yields region key
  `{1, 2}` over the whole cube (R-A3), and the D-41 items — the sheet box-clip curve and the box
  tag gated on the primary tag — are closed in the same pass. *Tier T2. Multimodal: no.*
- **M-4.1.** Moved to **M-1.6** (MG-12: M-3 waited for it, and it waited for M-3). The
  subtask text there supersedes v3's rule (a), which MG-11 refutes on paper.
- **M-4.2. The T-junction repair loop.** After M-2 and M-1.6, count how often the detector fires
  on the matrix. If zero, delete the repair (a repair that never repairs is a hedge); if not, each
  firing is a J1 violation upstream to fix at its source, and the loop stays only until that count
  is zero.
  *Acceptance:* the count, and either the deletion or the named upstream defects. *Tier T2.
  Multimodal: no.*
- **M-4.3. The thin fixtures do not exercise the thin path.** Measured today (§2.2): a6a, a6b, a7a
  and a7b — built to put a limb or a gap in the sheet and band regimes — report **0 sheet faces and
  0 band elements** on the default path, while a3, which has no thin feature, carries 514 band
  elements at the lens tip. R-B1/R-C1 are therefore untested by the suite. Decide per fixture from
  the S3/S8b diagnostics whether the regime was never detected, detected and declined, or detected
  and correctly meshed volumetrically; make the fixtures exercise their regimes at their committed
  settings, and make `[V7]` non-vacuous (a sheet-regime case must report sheet faces or FAIL).
  *Acceptance:* `[V7]` metrics non-zero on the four thin cases as their regimes intend; a3's 514
  band elements explained or removed; `collapse_sheets` either becomes the adaptive default or is
  deleted (R3 — a setting choosing whether a sheet collapses is a correctness knob).
  *Tier T3. Multimodal: yes — cross-sections of the limb and the gap, which is what the fixture
  generator promised and never shipped.*
- **M-4.4. P1 campaign.** Build the never-built fixtures: A-3 ranked (R-A4: two keys, the lens to
  the winner — no case has ever had two priorities), A-4b, A-5, A-9 (lattice ∩ sphere); and the
  three §2.7 found missing: **A-11** an open sheet crossing the box that partitions it (R-C1, R-C3,
  R-D2 — the sheet path and `[V8]` have never run on an acceptance case), **A-12** a non-cubic domain
  (R-D1 is untested while every domain is `[0,1]³`), **A-13** a defective input (a duplicated
  triangle, a flipped one, a small gap) through each `repair` level (topic M has 19 unit tests and
  no end-to-end run). Run the real datasets under `workspace/data` — the yarn and the four WAAM
  builds — through S0–S8 as **smoke inputs**: no hard failure, `[V1]`/`[V3]` PASS, and every FAIL
  charged to an arm. Add a deterministic placement sweep (a3's six placements, record §9154) as a
  robustness test of the arrangement/lattice alignment (A-§9154).
  *Acceptance:* every input meshes; the new fixtures join the matrix with their expected-outcome
  records (A-3-ranked: exactly two non-background keys and the lens carries only the winner;
  A-11: `[V8]` partitions = 2 with the sheet's rim conforming; A-12: no cell outside the box after
  S11; A-13: the repair log names each action and `[V1]`/`[V3]` PASS); the WAAM/yarn runs are
  recorded with time and size. *Tier T2 (fixtures, T1 for the runs). Multimodal: yes — one render
  per new fixture.*
- **M-4.5. Input decimation as conditioning (D-1: yes).** An S0 `repair` option
  `decimate: <chord tolerance in model units>` that simplifies each input surface within the stated
  tolerance (edge collapses that keep every vertex within it of the original, feature curves and
  sharp corners pinned, closedness preserved) before S1. The conditioned surface is what `s00`
  writes and what P3 is measured against; `[V13]` additionally reports the boundary against the
  *original* input so the tolerance the user chose is visible in the report. It is input
  conditioning the user chooses per geometry — it changes the input, not the cut — and its
  default is **off** (no decimation), which is not a correctness knob: both settings produce a mesh
  exact to its input.
  *Acceptance:* on the three reference cases decimated to the chord error the reference tool's own
  mesh exhibits, the element ratio and on-surface share are re-measured and recorded beside §2.3;
  `[V1]`–`[V3]` PASS; feature curves of the decimated input are the original's within the tolerance.
  *Tier T2. Multimodal: no.*
- **M-4.6. Resolution derived from the geometry — the three `auto` combinations and their options
  (G1, R-E1, R-E4, R-E6; rewritten rev 3.3).** M-1.8 states the ladder; this subtask derives
  whichever half the user leaves as `auto`, makes `background: auto, max_level: auto` the default,
  and builds every value of D-10's options (e), the recommended answers as defaults. It follows M-3
  (R-E1: refinement only toward what the one path can represent) and M-4.3 (the thin regimes must
  fire before a policy about them can be measured), and M-4.9's fixtures are its thin test bed.
  (a) *The requested field.* The unclamped graded field `h*(x) = min_s (h_s + β·|x − s|)` — G4-1's
  formula without the clamp — over the **admitted** requests, each input's own level (M-1.8)
  applied to its sources first. Curvature/chord, feature and corner requests are intrinsic to the
  geometry already. The curve request is `h_max / curve_cells` (`sizing.rs:699–770`) — defined by
  the background it would help choose — so it is redefined intrinsically, or removed, by
  re-measuring `curve_cells` under the one path: it was set when the fan could not represent
  curves, and R2 makes refinement toward what the cut cannot represent pure cost. Gap and LFS
  requests are admitted per the thin options in (e). K1's requests and the band ladder's `RefineLocally` are
  admitted only where M-2.5 measures that they still bind with the fan gone — X-2 closed
  refinement toward the sub-cell body on the old kernel. v3's rule "every body at least four cells
  across" becomes a **local** request at that body, never a global bound (A-4's cube read 25.5 %
  short at 2.7 cells on the fan path); whether it is still needed once M-2's kernel cuts small
  bodies exactly is measured, not assumed.
  (b) *The finest need.* `h_need = min_x h*(x)`, floored at the smallest finest edge the envelope
  rule admits; a request below the floor is clamped and named in the report.
  (c) *The combinations.* **Background given, level derived:** `L = max(0, ⌈log₂(h_bg / h_need)⌉)`, lowered
  only when the pre-flight count exceeds a budget, and then with every request left clamped named.
  **Level given, background derived:** `h_bg` is the largest domain-fitting edge
  `≤ min(h_need · 2^L, h_far)`, so the finest level lands at or below the finest need; if the
  pre-flight count exceeds a budget the run is refused, naming the level that would fit — a given
  level is never silently coarsened. **Both derived:** `h_bg` is the largest domain-fitting edge
  `≤ h_far`, and `L` as in the first. **Both given:** M-1.8, with this subtask's analysis printed
  beside it in report-only form — what the geometry asked for, and which requests the given ladder
  clamps.
  (d) *Domain fit.* The largest edge `≤` the target at which every extent is an integer multiple
  (within the weld quantum); if none exists at or above half the target, the longest axis fits
  exactly and the others overhang by less than one cell, reported.
  (e) *The options (R-E6; owner decision D-10).* Three keys, each a policy the geometry cannot
  supply, every value built and tested, the recommended answer the default.
  `sizing.thin_material: resolve | template` (default `resolve`): under `resolve` a thin material
  region (`intra(X)`) drives the derived level until it is out of the sheet regime, after which
  the band ladder decides band or volumetric (its `RefineLocally` may deepen the level further),
  and it never collapses; under `template` it does not drive the level, takes the regime the
  shape-driven field gives it, and below `t_sheet` collapses to a sheet whose thickness is kept
  (`SheetThickness`, M-6.7) — a shell-like idealisation the user chose, its departure from the
  two walls reported by `[V7]` as D-6 recommends. `sizing.thin_gap: template | resolve` (default
  `template`): under `template` a thin gap (`inter`) does not drive the level below the
  shape-driven field and the band/sheet path takes it, exported per `output.interface`; under
  `resolve` it drives the level until it is `gap_cells` across, bounded by the envelope floor and
  the budget, and whatever those stop is handed to the thin path and named.
  `sizing.far_field: capped | uncapped` (default `capped`) sets `h_far`, the far-field edge — the
  one scale no geometric source supplies, because nothing in the far field needs resolving:
  `capped` is `0.05` of the domain diagonal (a named constant, today's `h_max_frac` default),
  `uncapped` is unbounded, so the domain fit alone bounds the background and the graded field
  shapes the far field. A given level cannot be deepened, so under a given level the thin options
  only report — a region the option would resolve and the level cannot is named with the level
  that would (M-1.8 (e)); `far_field` beside a given background is a parse-time reject, the
  background being the cap. R3 is answered by D-10 as D-4 answered it for the interface export:
  each value is a modelling or cost policy, and the one that departs from the input on purpose —
  `thin_material: template` — says so in the report and in `[V7]`, never silently.
  (f) *Preconditions restated.* Every rule written against `h_max`, `h_min` or absolute octree
  levels is restated against the ladder, each with a test: D-38's balance clause when the
  background is one or two levels deep; the S3↔S4 start `h⁽⁰⁾ = h_max`; S3's bootstrap and speck
  scale; `lfs_floor`; K1's clamp.
  *Acceptance:* the nine cases and the three reference cases run with both keys `auto` and **no
  per-case overrides**, each reproducing or beating its M-1 row at equal fidelity (M-1.4's rule) —
  A-4, A-6 and A-8 no longer carry hand-set values, except a fixture whose purpose is a regime
  the default options do not choose, which sets the option or keeps explicit keys and says why;
  on a3 and a8 each
  one-sided combination satisfies (c)'s relation (the derived level is the smallest that meets the
  need under the given background; the derived background puts the finest level at or below the
  need); a budget refusal and an envelope clamp each have a negative fixture; every run's
  `[S4/RES]` line names the request that set `L`; every option value is exercised on the fixtures
  it concerns, with its documented outcome — `thin_material` on A-6a, A-6b and A-17m..A-20m
  (`resolve`: no material sheet face and the body's volume within contracts §5; `template`: a
  sheet exactly where the analytic thickness is below `t_sheet`, within one cell, with
  `SheetThickness` populated), `thin_gap` on A-7a, A-7b, A-10 and A-17g..A-20g (`template`: band
  and sheet where the gap field says; `resolve`: volumetric down to the floor, the element cost
  reported), `far_field` on a1 and A-12 (the far-field edge at `h_far`, or at the domain fit);
  one negative fixture per reject (an unknown value, `far_field` beside a given background); the
  fractions are removed from the acceptance configs and then from the parser (a reject naming
  `background`/`max_level`); A-16 closed with its numbers. *Tier T3. Multimodal: no.*
- **M-4.7. The forging fixture and the G4 gate.** **A-10**: two flattened solids (ellipsoids, or
  two `a1` spheres pushed through the repository's own `forge` pipeline) whose opposing faces close
  from a wide gap to contact at the centre, so one part spans **volumetric → band → sheet →
  contact** in a single mesh. This is the geometry the owner's forging workload produces and the
  case R-B1/R-B2 were written for.
  *Acceptance:* in one mesh, the outer annulus is volumetric, the band annulus is **one** element
  layer (`[V7]` one-layer clause non-vacuous, band AR/dihedral inside the ladder's gates), the
  sheet annulus is a collapsed welded sheet with its rim curve declared, and the contact disc is a
  marked interface: every shared face tagged for both bodies with `FaceTagSideElems` populated;
  the regime boundaries land where the gap field says (`t_sheet`, `t_layer`) within one cell;
  `[V1]`/`[V3]`/`[V13]` PASS. A committed cross-section render. *Tier T3. Multimodal: yes — the
  cross-section through the contact centre.*
- **M-4.8. `FaceTagOrientation` per member, and one side convention (MG-07).** `cut_to_doc`
  pushes one `FaceTagComponents` entry per tag of a face and one `tag_orientation.push(1)` per face,
  so on two cubes in contact at `x = 0.4` the table has 280 members and 270 orientations (contracts
  §2.3: one `±1` per flattened member) and every value is `+1`; `FaceTagSideElems` takes the first
  tag's `(inside, outside)`. Fix: per member, `±1` derived from the tag's oriented patch against the
  canonical emitted face; for a multi-tag contact face one geometric plus/minus side, with each
  component's inside/outside mapped onto it, so a solver reading `(elem⁺, elem⁻)` knows which body
  is which; C2's opposite-orientation identity preserved. *Acceptance:* lengths equal; two opposed
  solids in contact carry opposite orientations; input order and node rotation leave the geometric
  side unchanged; M-1.0's validator rejects the pre-fix document. Prerequisite of M-4.7 and M-6.6.
  *Tier T2. Multimodal: no.*
- **M-4.9. The thin family: tilted, irregular and non-uniform thin regions (R-B3, the owner's
  requirement of 2026-09-25).** Every thin fixture today is an axis-aligned box of uniform
  thickness — A-6a/A-6b's limbs and A-7a/A-7b's plates (`generate_acceptance_cases.py:96–104`) — so
  the thin path has only been asked about slabs whose walls are lattice planes, whose outline is a
  rectangle and whose thickness is one number. S3's unit tests measure an oblique and a curved gap
  (`tests/meshgen_gapfield_tests.rs:248`, `:287`), but neither reaches S8b; a thin path that worked
  only on lattice-aligned slabs would pass the whole suite. **Fixtures** — each a committed
  analytic generator that writes the STL and its config from one parameter set, thicknesses stated
  as multiples of the fixture's own finest edge so no regime drifts out of its window when a
  default moves (the "`t_sheet` and `t_layer` are fractions of the converged `h`" lesson, which
  cost A-6a/A-7a two re-tunings); variant `m` is one thin plate of material in a matrix box, or
  rooted on a block, and variant `g` is two thick bodies whose facing walls are the same plate's
  two faces, the gap between them empty:
  - **A-17m / A-17g — tilted.** A flat plate of uniform thickness at five orientations: lattice-
    aligned (the control); 17° about one axis; 45° about one axis; a generic rotation (17°, 29°,
    41°); and walls lying on a lattice facet plane (`x + y + z = const`, the checkerboard's own
    facets after M-1.7 — the coincidence case, like a2's walls on axis planes). One thickness in
    each regime window — sheet, band, volumetric — at the fixture's given ladder.
  - **A-18m / A-18g — irregular.** A curved mid-surface (a saddle `z = κ(x² − y²)`, and a
    cylindrical patch) with a non-convex outline — a lobed polygon with a re-entrant corner and a
    hole — at uniform thickness.
  - **A-19m / A-19g — non-uniform.** Thickness varying inside one body: a linear taper from above
    `t_layer` to below `t_sheet` with a sinusoidal ripple on it, so each regime boundary is crossed
    more than once; the gap variant is a wedge between two flat blocks closing to contact along one
    edge, and a rippled gap. A-10 is the curved, forged version of the gap wedge; A-19g is its
    flat control, measurable against a closed-form thickness map.
  - **A-20m / A-20g — all three at once.** A tilted, curved, irregular, tapered flake — the shape
    real powder and WAAM porosity produce — as material, and as a gap of the same shape between
    two bodies.

  **Runs.** Each fixture at its given ladder, so the regime windows are known, and under derived
  resolution with each value of the `thin_material` / `thin_gap` option it concerns (R-E6, M-4.6).
  The generators may land as early as M-1 and join the matrix as soon as they exist — a fixture
  that fails is still a measurement — but the gate below is read after M-4.3 makes the thin path
  fire at all.
  *Acceptance:* `[V1]`/`[V3]` clean on every run, across every rim and every regime transition;
  **orientation invariance** — A-17's four tilted orientations agree with the lattice-aligned
  control on each regime's area share, the plate's volume error and elements per unit area,
  within the tolerance one cell of rim explains, stated in the expected-outcome record, and the
  facet-plane orientation meshes without a coincidence defect (`[V2]` clean — record §6.64's class
  on purpose); A-19's regime boundaries land where the analytic thickness crosses `t_sheet` and
  `t_layer`, within one cell (A-10's rule); A-18's rims follow the re-entrant corner and the hole
  with the rim curve declared, and S3's mid-surface validates or its refusal is named; `[V7]`
  non-vacuous wherever a band or a sheet is expected, `[V13]`/`[V5]` against the analytic
  geometry; each option value shows its documented outcome (M-4.6's acceptance). A failure is
  charged to its arm and owned by M-4.3 (S8b), S3 or M-4.6 — this subtask builds and measures, it
  does not tune. *Tier T2 (generators and runs) / T3 (diagnosing an orientation dependence).
  Multimodal: yes — a cross-section through each plate and gap, and A-17's five orientations side
  by side; an orientation dependence is visible before it is countable.*

### M-5 — Quality (P4)

`[V4]` is WARN on every case on both paths, with minimum dihedrals down to `4.3e-5°` and aspect
ratios to `2.6e12` (§2.2). The record's one measurement of where the badness lives says it is
two-dimensional first: on a1, 94.3 % of tets below 10° inherit a needle *face*, frozen by J1, that
no three-dimensional operation can reach (record §6.54). So the order is face, then trace, then
interior.

- **M-5.1. The quality census.** `[V4]` charged to `plc_path` and to "inherits a needle face" vs
  "true sliver" on all twelve cases (M-1.1's census extended), plus the never-measured G4-3 post-snap
  transition-cell quality (SPEC geometry §3.7 predicts `35.264°`/`AR 1.6052` pre-snap under Rule D,
  and `45.000°`/`AR 1.5607` lattice-wide once M-1.7's checkerboard lands, its plain leaves at
  `54.736°`/`AR 1.3660` — Appendix D.1). D-5 (yes):
  the reference-verbatim SAMR fallback stays in the design, and this measurement is what finally
  gives it a trigger — G4-3 fails only if the transition cells, not the cut, are what breaks `[V4]`.
  *Acceptance:* the table; the top three producers of below-10° tets named per case; G4-3's verdict
  recorded with its number. *Tier T2. Multimodal: no.*
- **M-5.2. Quality face triangulation inside the J1 cache.** The `FaceTriCache` computes a face's
  triangulation once from the face's keys and constraints; a bounded quality refinement of that
  triangulation (Steiner points on the face computed as exact averages of face points, welded within
  `q`, deterministic in the face's own frame) is a pure function of the face and therefore
  J1-legal — §7.4's "no Steiner on a shared face" binds the *interior* mesher, and M-0.2 says so (geometry rev 1.6 §7.4 carries the note).
  The cost rule is X-5's: the record refuted an unbounded conforming refinement at 4 s → 10 min, so
  this one is bounded per face and measured on a6a's wall time first.
  *GO if:* below-10° share falls by half on a1 and a6a at ≤ 10 % more elements and ≤ 25 % more
  time; *NO-GO* recorded otherwise. *Tier T3. Multimodal: no.*
- **M-5.3. Near-edge trace points — constrained (re-scoped under MG-11).** The needle faces come
  from trace endpoints a continuum of distances from a face's edge or corner (record §6.63). v3
  proposed collapsing such a point onto the edge because the move is "small enough that `[V13]`
  does not see it"; that shows the old measurement would not alarm, not that the point is on the
  surface, and the move can shift an intersection curve or merge two entities the arrangement keeps
  distinct — under MG-02's gap the operation would have been *rewarded*. Re-scoped: a trace point
  may be replaced by a point on the edge only when that point is also on the constraining surface —
  the edge's own exact crossing with the patch, i.e. the trace endpoint the edge itself would have
  produced — so the operation is an identity change with an exact geometric justification, and
  otherwise it is not done; the needle is then S9's (M-5.4) to remove by an interior operation, or
  M-5.2's by a face refinement. *Acceptance:* needle-face count on the matrix; every replaced point
  at distance exactly 0 under M-1.5's containment; `[V3]` held; `[V13]` unchanged to `1e-6`.
  *Tier T3. Multimodal: no.*
- **M-5.4. S9 — interior improvement.** Round-based IQD as designed (record §10.12), restricted by
  P3: flips and Steiner insertion only in the interior; smoothing moves **free nodes only** (a node
  on a tagged face or a curve never moves — the constraint arrays exist for this and are `Free` for
  every cut node today, which M-5.4 must fix first: a cut node's constraint is the surface it was
  interned on); hole-fill with majority records; strict-mode CPU recompute of every commit.
  *Acceptance:* `[V4]` PASS on all twelve at the §4 defaults; `[V13]` unchanged to `1e-6`;
  `[V1]`/`[V3]`/`[V6]`/`[V9]` PASS; R-P2; elements within M-1.4 + the Steiner count reported.
  *Tier T3. Multimodal: no.*

### M-6 — The deliverable: S10, S11 and validation

- **M-6.1. S10 as a stage.** Label resolution, `N_ID` and the sheets taxonomy exist inside S8's
  `cut_to_doc`; partitions do **not** — `cut_to_doc` writes `partition_id` as `vec![0; cells]`,
  without even the non-tet sentinel (MG-13), so the `[V8]` flood fill of B.1 is *built* here, not
  moved. The stage runs after S9 (and after the trim, M-6.2) on the final population, with
  `[V6]`/`[V8]`/`[V9]` run there. Welded-sheet semantics as B.6. **Snapshot:** none of its own —
  the stage enumeration is frozen (`9` thin, `10` quality, `11` final; contracts §2.4/§3,
  `snapshot.rs`) and v3's `s10_regions` collided with `s10_quality_r<N>` (MG errata); S10's tables are
  part of `s11_final`. *Acceptance:* matrix unchanged; `partition_id` carries `−1` on non-tets and a
  recomputed flood fill on tets; `[V8]` non-vacuous on A-11. *Tier T2. Multimodal: no.*
- **M-6.2. S11 — trim before quality, then export (MG-13).** The lattice overhangs a non-cubic
  domain by up to one coarse cell per axis (B.7), and v3 scheduled the trim *after* S9 and S10. A
  box plane through a tet creates new nodes and sub-tets, can leave arbitrarily thin slivers, and
  changes element ids, side pairs, `N_ID` sets and the partition graph — so an S9 PASS and S10's
  tables measured before the trim do not describe the exported mesh; deleting crossing tets leaves a
  gap, and copying parent metadata rebuilds nothing. **Decision (this plan): the domain box is an S8
  constraint.** The six box planes are cut like surfaces — the way G2-5 already caps the surface
  against them — so after S8 no tet crosses the box; S11's trim is then the deletion of whole cells
  outside it, creating no node, and `[V3]`'s hull relaxation ends at S8 (M-1.0's MG-03 fix). If the
  box cut is measured to produce slivers the §4.4 ladder cannot clear (A-12's sweep below), the
  alternative is a geometric trim at S11 followed by a full re-run of S9, S10 and every derived
  table; under either, the export serialises only a state that has been through quality, labels,
  partitions and verification **after the last topology change**. Then: Abaqus INP (`C3D4`, ELSET
  per region key and partition, NSET per component, `*SURFACE` per interface pair from
  `FaceTagSideElems`) with B.7's material-mapping policy — unmapped multi-ID keys are a hard error;
  `[V10]` implemented; `mesh` exits **0** and writes `<name>.vtu`, `<name>_contract.vtu`,
  `<name>.inp` and the report. *Acceptance:* on A-12 and a sweep of domain planes within ULPs of a
  lattice vertex, `[V4]`, `[V6]`/`[V8]`, side-element references and `Counts` are consistent after
  the trim, the tet volume fills the box and no cell lies outside it; `[V10]` PASS on the matrix;
  the unmapped-key fixture fails with the key list; a solver reads the INP (M-6.4). *Tier T3 (the
  box cut) / T2 (export). Multimodal: no.*
- **M-6.3. `[V11]` compare mode.** `mesh-verify --compare a.vtu b.vtu --mode strict|topology` as
  the contract states; used by M-1.2's byte-identity gate from then on. *Tier T2. Multimodal: no.*
- **M-6.4. Validation on exported meshes** (the record's GT-1..GT-4, GT-6): the synthetic suite
  (record §17.3) green twice; reference cross-validation with the compat mapping and per-metric
  diffs; a FEM smoke test — a minimal linear-elastic assembly on the exported INP in a solver the
  repository can drive, or a committed script that assembles a Laplace stiffness matrix from the
  INP and checks it is symmetric positive definite; visual acceptance (cross-sections of every
  semantic situation vs the reference renders); and the A-1..A-9 suite on **exported** meshes with
  analytic volumes and areas within contracts §5.
  *Acceptance:* §1.2's gate, every clause. *Tier T2. Multimodal: yes — cross-sections and
  side-by-side comparisons; the defects sought are visual.*
- **M-6.5. Performance envelope.** GT-5: time and peak memory per stage per case against record
  §14's envelope, on the identified host; the largest reference case and A-9 included. The budgets
  M-1.3 set become the committed table.
  *Acceptance:* the table; any stage over its envelope has a named cause. *Tier T1. Multimodal: no.*
- **M-6.6. The interface export is the user's choice: `output.interface: welded | split` (D-4),
  with `split` specified as node-star sectoring (MG-14).** A marked interface in B.6's taxonomy is
  welded — C0, one node set — with `(elem⁺, elem⁻)` per tagged face so a solver can insert cohesive
  elements or a contact pair without re-deriving adjacency. v3 specified `split` as "duplicate the
  interface face nodes and remap `elem⁻`"; that is wrong at every node also used by same-side tets
  not adjacent to the face (they keep the old node and the side is inconsistent), fragments a
  continuous crack face by face, and "two copies per node" fails where three interfaces meet along
  a curve, at multi-material junctions and at a crack front. Specified instead: (1) freeze the
  **selection** — which component pairs / tagged faces are cut open — and the ± side convention
  (M-4.8's geometric side); (2) for every node incident to a selected face, build its incident-tet
  star, remove adjacency across the selected faces, and allocate **one copy per connected sector**
  (two on a plain two-body contact, more at a junction, one at a crack-front node), remapping every
  tet of each sector at once; (3) the crack front (a selected face set ending inside the domain),
  intersecting cracks and multi-material junctions have stated rules and expected copy counts; (4)
  the INP's split node numbering is an explicit map from the welded VTU ids, exported beside the
  NSET pairs, and `[V10]` verifies through that map — never by count equality. Both exports are
  correct meshes of the same geometry — the choice is what the downstream solver wants, like
  `fem_profile`, not a correctness knob (R3): a config field, default `welded`, validated (`split`
  requires at least one tagged interface and names the components it splits). The VTU contract is
  unchanged: the contract document stays welded and carries the side-element pairs; the INP carries
  the split. Under R7, `split` is the one condition under which duplicate nodes and an extra feature
  edge are permitted, and `[V2]`/`[V3]` on a split export exempt exactly the copies the map lists.
  *Acceptance:* on A-10 and a6a, `welded` produces one node set and the side-element pairs; `split`
  on a planar two-body contact, an internally terminating crack, a closed interface and a three-way
  junction produces sectors each `[V3]`-clean, unselected interfaces still connected, every copy
  traceable to its original; Feature Edges show the box and the crack's rim only; a solver reads
  both INPs. *Tier T3 (the sectoring) / T2 (export). Multimodal: yes — Feature Edges of the split
  export, which is how the owner states the rule.*
- **M-6.7. Sub-`t_sheet` gaps as a cohesive layer: `output.interface: cohesive` (D-8, §12.1).**
  S8b retains, per collapsed node, the two wall crossings G7-2 averaged (`SheetPairOffset`, a
  6-component `Float64` point array: offsets to wall⁺ and wall⁻, zero off-sheet) and, per sheet
  face, its thickness (`SheetThickness`, `Float64` face-cell array, `−1` off-sheet; both frozen in
  contracts rev 1.4, S-52). The VTU stays welded. At S11, `cohesive` runs M-6.6's sectoring on the
  sheet faces, moves each copy to its own wall by the retained offset — each move passing S7's
  exact no-inversion test on every incident tet (the motion is `≤ t_sheet/2 = 0.1·h`, inside the
  `0.30·l_min` envelope S7 already works in) — and emits one 6-node cohesive wedge per sheet face,
  bottom triangle on elem⁻'s copy, top on elem⁺'s, stacking direction fixed by M-4.8's side
  convention, one element set per component pair; the section takes its thickness from the
  geometry and its constitutive response from `fem_profile`. A copy that cannot reach its wall
  without inverting a tet stays at the midpoint and the wedge carries `SheetThickness` as a
  specified thickness instead — reported per face with the shortfall, never chosen by a setting
  (R3; D-8a). At the rim the copies coincide with the band layer's own wall nodes, so the wedge
  tapers into the band with no new rule; at a contact (`t ≤ ε`) the wedge is D-4's split with a
  zero-thickness element. Verifier: `[V7]` requires `SheetThickness ∈ (ε, t_sheet]` on every sheet
  face; `[V10]` through M-6.6's map requires wedge count = sheet faces, both wedge triangles within
  `[V13]`'s fit tolerance of their walls (against the effective surface, D-6), stacking direction
  agreeing with `FaceTagOrientation`; `[V4]` excludes cohesive elements by type; `[V5]` reports
  `Σ area·t` as the gap's `inter` volume and reconciles it against the input gap. R8 negatives: a
  sheet face thicker than `t_sheet`; a copy left at the midpoint without a report; a reversed
  stacking. `collapse_sheets` becomes the Sheet-regime default and the flag is deleted (M-4.3).
  *Acceptance:* A-7a and A-10 under `cohesive`: `[V3]` clean across every rim, every wedge on both
  walls or reported, gap volume within contracts §5 (A-7a's is analytic), B.8's FEM smoke solves
  with a stiff traction–separation law and a continuous displacement field up to the layer's own
  compliance; `welded` and `split` on the same meshes unchanged to the byte. After M-4.3 and M-6.6,
  before M-6.4's FEM smoke. *Tier T3 (retention and placement) / T2 (export). Multimodal: yes — a
  cross-section through the gap showing the wedge between the two walls.*

### M-7 — GPU (carried unchanged, last — after the certificate is repaired)

GK-1 (`mg_broadphase` + `mg_rays`, margin-certified), GK-2 (`mg_field`/`mg_flags`/`mg_quality`,
proposals only), GK-3 (determinism harness, cross-device), GG-1 (real-hardware validation with the
adapter asserted). Text and acceptance as B.10; each starts strictly after its host stage's CPU
path passes M-6.4. **Precondition (MG-09):** certificate G1 as frozen bounds the arithmetic on
*given* f32 inputs; the f64→f32 conversion of the inputs is not in it, and G2's inflation covers
only the broad-phase boxes. On `a = (0.1, 0.1, 0.5 − 2e-8)`, `b = (0.5, 0.1, 0.5 + 2e-8)`,
`c = (0.1, 0.5, 0.5 + 2e-8)`, `p = (0.15, 0.15, 0.5 − 1.1e-8)` the f32 determinant in G1's own
expansion order is `−3.576e-9`, the bound `1.554e-15`, the filter accepts — and the exact
determinant of the f64 inputs is `+1.600e-10`. Numerics rev 1.3 records this (§11 D-13) and
withdraws G1's "sign is exact" claim for converted inputs; before GK-1 lands, numerics rev 1.4
(M-0.2) must either add the conversion term to the filter bound (an interval on each converted
coordinate, propagated through the same expansion) or restrict every GPU emission to a candidate
superset that can never exclude, with all sign-bearing decisions re-made on the original f64
coordinates; if local-origin upload is used, the local-origin subtraction's own error is bounded the
same way; G3 separates conversion, per-triangle solid-angle and reduction error. GK-1's parity
tests compare against the **original** f64 geometry, never against a reference itself rounded to
f32, and the cross-device and strict-parity gates include this degenerate neighbourhood. GPU
candidate ordering never decides commit order without CPU canonicalisation. **Starting point
(numerics §6 as-built):** no meshgen GPU kernel exists at `891badc` — `src/gpu/` holds the S2,
voxel, transform and render pipelines only; the CPU GWN accumulates no `S` and `gwn_margin_band`
is dead code at `2⁻²³` (numerics D-25); the S2 broad phase is the CPU f64 hybrid grid with no
`δ_bp`. GK-1 is built from the certificates, not from a partial kernel. *Tier T2 (T3 for the
certificate). Multimodal: no.*

### M-8 — Documentation and the record

- **M-8.1. English docs.** An algorithm page for S8 (the trace, §5.2, §6, §7.2–§7.4, the
  facet-split fan, the declarations) — today `cut.rs` and `cdt.rs` (15,806 lines) have no algorithm
  page and 36 function-index rows between them; reference entries; the meshgen section of
  `AGENTS.md` rewritten to cite this plan and the specs rather than restate them. Stale statements the review found in the English reference, to be corrected there (not
  copied into a spec as normative without a check against the code): `docs/en-us/reference/meshgen.md`
  gives `gap_cells` a default of `2.0` at three sites (the `MeshGenSizing` row, the config
  example, the defaults table) against the code's `4` with a parse-time floor of 4;
  `docs/en-us/reference/mesh-verify.md`'s catalog row still reads "`[V9]` … skipped — lands with
  G6-4" although `[V9]` has run since 2026-08-13; and the S8 narrative's "S7 + S8 are what make
  the boundary exact" is the design statement, which the P3 measurements of §2 qualify; and
  the G6-6 table's post-cut row (`meshgen.md` ~1914, zh-cn ~1051) reads `0.874 / 5.522 / 45.000`
  and `1.848 / 6.474 / 45.000` where the same test at `891badc` prints `0.243 / 5.268 / 45.000`
  and `2.000 / 5.542 / 44.994` (populations 5,844 / 4,360) — stale numbers under a GO
  conclusion that still holds. The
  reference has no entries for `facecache.rs`, `check_face_cache` or `orient_face_outward`, and
  `cdt.rs:45–47` and `cut.rs:2925–2932` still say the face cache is not implemented / consumes
  nothing (geometry rev 1.6 D-22). *Tier T2.
  Multimodal: no.*
- **M-8.2. Chinese mirror** per `TRANSLATION_GUIDE.md`. *Tier T1. Multimodal: no.*
- **M-8.3. This document's own upkeep.** The record is gone (D-2 and the owner's instruction of
  2026-09-01) and so is `PLAN_MESH_GEN_REVIEW.md` (merged here 2026-09-23); this file is
  `PLAN_mesh_generation.md`, committed once on 2026-09-24 at the owner's instruction (its
  `.git/info/exclude` line stays; a tracked file ignores it). **When every subtask in §11 is done,
  this file is deleted from the repository** — the owner's rule of 2026-09-24: the three
  `SPEC_*.md`, `AGENTS.md` and `docs/` carry everything durable, and a finished plan left behind
  becomes a second, stale record. The deletion is the last act of M-8.3 and is committed with the
  sentence "plan complete, destroyed per §M-8.3". Every phase appends its measured rows
  to §2's tables and its refutations to §6.4, in place — no second plan document, no status log
  growing underneath. Appendix A is closed: new findings go into the sections they belong to.
  *Tier T1. Multimodal: no.*

---

## 8. Contracts — the spec revisions this plan requires

| revision | when | content |
|---|---|---|
| geometry rev 1.6 | M-0.1 — **landed 2026-09-23** | §7.7 as-built; §15 rows for the fan (to be removed), `cut.rs:14–23`, FaceTriCache (status and the counted-not-raised mismatch, MG-06), X-per-file (MG-04), borrowed closure (MG-05); §15 G6-0 item closed; `[R1]` text in §0; the plan cross-reference map |
| numerics rev 1.3 | M-0.1 — **landed 2026-09-23** | §1.5 the four roles (identity key, ordering key, coordinate, error metric — MG-11); the as-built named constants; G1's conversion gap recorded (MG-09, D-13); §11 open items updated (the junction CDT exists) |
| contracts rev 1.3 | M-0.1 — **landed 2026-09-23** | `[V13]` FAIL at 1.0 as normative with the as-built WARN recorded; the containment criterion stated as P3's definition with the corner test as its necessary half (MG-02); the item-cap rule, the stage-11 domain rule and contract validation as normative with the as-built defects recorded (MG-01, MG-03, MG-08); `[V6]` undeclared-boundary clause + mixed-priority exemption; the code-only tolerances frozen; `[V5]` interface-node set restated; diagnostic arrays listed as non-contract; per-member orientation and `partition_id` as-built deviations (MG-07, MG-13); the fixture table brought to fourteen files |
| geometry rev 1.7 | M-1.7 and M-1.8 (rev 3.3), after M-1.1 and before M-1.2 | §2.2 gains the 5-tet parity checkerboard (both parities, volumes `h³/3` and `h³/6`) as the plain-leaf template (D-9), the Freudenthal table kept as history; §2.3 Rule T5 with the corrected rationale (S-53); §3.3 cases P and Q; §3.4 Bound P1's upper bound `h³/3`; §3.5's plain row and the budget `5·N_plain + 48·N_fan`; §3.6's proof text; §3.7 re-measured (plain row `54.736° / 90.000° / 1.3660`, Q row `45.000° / 90.000° / 1.3938`, lattice-wide worst `45.000° / 120.000° / 1.5607`); §3.1's as-built root and forest rule for arbitrary background counts and per-input levels (S-54). M-4.6's derivation (§3.1, §11's `h⁽⁰⁾`) joins the first geometry revision after it lands |
| geometry rev 1.8 (was 1.7 before rev 3.3) | M-0.2, after M-2 and M-5.2 | §7.4 Steiner-on-facet rule (M-2.1 as measured); the trace-point rule for facet vertices on a face (M-2.2); §7.6 = the facet-split fan, whole-cell fan absent; face-cache quality refinement (M-5.2) as a J1-legal pure function |
| numerics rev 1.4 | M-0.2, after M-1.6 and before GK-1 | the identity rule M-1.6 measured (never "quantise to `q`" as v3 wrote it — MG-11); certificate G1 with the conversion term, or the superset-only rule (MG-09) |
| contracts rev 1.4 | M-0.2 | `[V9]` curve carriage FAIL; `[V10]`/`[V11]` as implemented; the split-export map (M-6.6); the resolution stamp M-1.8 writes as non-contract FieldData until then (given/derived per quantity, `h_bg`, realised counts, `L`, per-input levels) |

Every revision: bump the rev line, add the §15 row, add the §14 record with its script, and pin
the value with a test (R6).

## 9. What is kept, and the invariants

Kept, and not to be re-derived: the exact-predicate foundation; S0–S2; S3; the frozen tables (§4
SNK, §4.3, §5.2, §6, §8.2; §2.2's six-tet table gives way to the 5-tet parity checkerboard for
plain leaves once M-1.7 lands, D-9); the §7.2–§7.4 kernel and the J1 face cache;
`[V1]`–`[V13]` and the corrupted-fixture manifest; the measurement record. **T1** survives M-1.7
unchanged in its proof: the rule it is proved with becomes Rule T5, which is a pure function of
the face at the face's own level exactly as Rule D was.

Invariants that survive any rewrite: **J1** (a shared face is triangulated identically by both
owners, from the face alone); **J2** (the generic face triangulator reproduces §5.2 where §5.2
applies); **K1/K2**; **B1**; **R-P2**; **sheets never claim volume**; and, new in this plan, **the
material boundary is declared** — every face whose two owners resolve to different region keys
carries a tag for each component in the difference, because `[V13]` reads the boundary off the
labels and the contract requires it declared; and **the verifier is trusted** (R8) — a gate is
cited only after its negative fixture has failed it.

## 10. Assumption register

**[V]** verified (measured, script cited) · **[P]** plausible, prototype-gated · **[U]** unsafe
without the stated fallback.

| # | assumption | class | evidence / fallback |
|---|---|---|---|
| A-1 | The §7.2–§7.4 kernel is exact where it runs | **[V]** a3 §7.4-meshed 0.4 % off-surface vs the declined arm's 87 % (record §6.49); a8 0.0 % (record §6.53) | — |
| A-2 | Steiner insertion on the facet converts the majority of §7.4's refusals | **[P]** it is the standard remedy for exactly these two refusal classes; the record's cone-in-the-link (X-7) is a different operation | M-2.1 gate; NO-GO keeps the facet-split fan |
| A-3 | With M-2.1 and M-2.2, every cell of the matrix is meshable without the whole-cell fan | **[P]** a8's 3,281 refusals split 54 % facet classes / 44 % hull classes, so both are needed | M-2.3 makes the residue an error with a dump, never a chamfer (R1) |
| A-4 | The gated path is byte-identical across thread counts | **[V]** on a1 (§2.6); **[P]** on the rest | M-1.2 measures; a mismatch blocks M-3 |
| A-5 | Quantising the input to `q` at S0 removes the one-ULP pairs without a visible displacement | **refuted on paper** (MG-11): the q grid is not the lattice grid — `0.5/√3` rounded to `1e-5` returns `0.5000084…`, off the lattice — and S0 keeps raw coordinates behind quantised keys, so the "invariant mesh nodes obey" is identity, not geometry; the margin `(q/2)/(0.02·h_local)` is not fixed | M-1.6 chooses the rule by measurement from candidates that keep lattice alignment (D-3) |
| A-6 | 94 % of bad tets are two-dimensional in origin | **[V]** on a1 (record §6.54); **[P]** elsewhere | M-5.1 measures all twelve |
| A-7 | A bounded quality refinement of the face cache is affordable | **[P]** X-5 refuted the unbounded form on cost | M-5.2's GO/NO-GO on a6a's wall time |
| A-8 | S9 smoothing can hold P3 exactly | **[U]** a node on the interface that moves off it is a P3 violation | free nodes only; constraint arrays made truthful for cut nodes first |
| A-9 | The domain trim is a no-op for cubic domains and required for others | **[P]** every current domain is `[0,1]³` | M-4.4's non-cubic fixture |
| A-17 | The octree-plus-templates lattice passes G4-3 post-snap, so the reference-verbatim SAMR fallback (kept, D-5) is never triggered | **[P]** `[V3]` clean everywhere says conformity holds; quality post-snap has never been measured | M-5.1 measures; the fallback is the named alternative if it fails |
| A-10 | The reference-dataset ratio holds on the one path at matched resolution | **refuted today**: 1.13× / 0.89× / 0.67× on the gated path against 0.42× / 0.45× / 0.39× shipped (§2.3); the count is bounded below by the input's tessellation under P3 — **and neither column was at the reference's resolution** (§2.3's rev 3.3 correction: 1.0625× / 0.9375× / 1.41× the reference's background edge, one level short on the third case) | M-1.8 re-runs at the reference's own background and per-section levels; M-1.4 compares at equal fidelity; owner decision D-1 |
| A-11 | The real datasets are within P1 | **[U]** never run; may be non-manifold or self-intersecting | S0 repair levels; the S6 winding-number path; M-4.4 records what fails and why |
| A-12 | The thin fixtures can be made to exercise S8b at their committed settings | **[P]** the generator's regime arithmetic says they should | M-4.3; if the volumetric result is correct at those thicknesses, the fixtures move, not the mesher |
| A-13 | The `[V9]` regression the gated path shows (§2.2) is the wedge M-2 closes, not a new class | **[V]** every failing node is interned, on the curve, in junction cells (record §6.66) | M-2.1's acceptance names `[V9]` explicitly |
| A-14 | One forging fixture can span all four regimes so the G4 gate is a single mesh | **[P]** the gap field is continuous and the regimes are thresholds on it; a gap closing to contact crosses both | M-4.7; if the ladder still declines (as on a7a), the refusal is charged to `FaceShape`/`UncutFaces` and M-4.3 owns it first |
| A-15 | Decimation within a stated chord tolerance keeps P3 exact against the conditioned input and moves the original by no more than the tolerance | **[V]** by construction if feature curves and corners are pinned; **[P]** that the reference tool's own chord error is the right tolerance to compare at | M-4.5 reports both distances |
| A-16 | `h_max`/`h_min` derivable from the geometry reproduce the hand-set per-case values | **[P]** A-4's 0.0125 and A-8's 0.006 were found by sweeps that the local-feature-size criterion should predict | M-4.6, written as a derived background and level (R-E4) under D-10's options at their defaults (R-E6); a given value is always honoured exactly (M-1.8) |
| A-18 | The corner test's PASS on a2/a7b and the near-100 % rows in §2.2 survive M-1.5's containment criterion | **[P]** the sphere's facets are the input's own and the cut caps lie in them; a2's faces lie on lattice planes | M-1.5 re-measures every row; the old values are not a baseline |
| A-19 | The three verifier defects (MG-01/03/08) hid no §2.2 FAIL | **[U]** MG-01 hides a FAIL only when a section's items exceed the cap; MG-03 fires only on points outside the box (every current domain is `[0,1]³` and the lattice overhangs it, so it *is* active on every case) | M-1.0 re-runs the matrix under the trusted verifier and names every row that moves |
| A-20 | Cutting the box planes as S8 constraints produces no sliver the §4.4 ladder cannot clear | **[P]** a box plane is axis-aligned; G2-5 already caps the surface on it | M-6.2's A-12 sweep; the alternative (trim, then re-run S9/S10) is specified beside it |
| A-21 | One copy per connected sector of a node's cut star is the complete rule for `split` | **[P]** it reduces to two copies on a plain contact and gives the right count at a T-junction by construction; crack fronts and multi-material junctions need the stated rules | M-6.6's four fixtures |
| A-22 | The G1 certificate can be repaired by an interval term on the converted coordinates without losing its `3.3e-6` uncertain rate | **[P]** the conversion error is `u₃₂·\|x\|` per coordinate and enters the permanent linearly; **[U]** the measured rate was taken without it | numerics rev 1.4 re-measures before GK-1 |
| A-23 | The checkerboard lattice moves no FAIL gate for the worse on the matrix | **[P]** its plain tets are better shaped than today's (54.736° against 45°, Appendix D.1) and its fans are no worse anywhere and better over Q faces; **[U]** the plain tets S7 snaps into and S8 cuts change shape — a regular central tet of twice the corner tets' volume, no body diagonal — and what S7/S8 do with them is unmeasured | M-1.7 re-runs the matrix after M-1.8 and charges every moved row, element counts included, to the change before it lands |
| A-24 | An `h_bg · 2^m` root trimmed by D-38's whole-subtree rule gives arbitrary background counts with T1 intact | **[P]** it is the mechanism the non-cubic domain already uses (`a_non_cubic_domain_stays_conforming`), applied to more subtrees | M-1.8's 17³, `[20, 20, 12]` and `L = 0` fixtures, `[V3]` on each |
| A-25 | Per-input levels reproduce the reference's per-section refinement closely enough for R2's matched comparison | **[P]** a per-input cap on the sizing requests is what a per-section level means; **[U]** the grading still refines near a level-0 input that sits beside a level-2 one, where the reference's per-section refinement does not | M-1.8 prints the realised finest edge per section beside the reference's; the residual difference is reported with the ratio, never folded into it |
| A-26 | The thin path's regime decisions do not depend on how a thin region sits against the lattice | **[U]** every thin fixture is lattice-aligned (M-4.9); S3's rays and sweep are direction-free by construction, but S8b finds band cells with the doubly-cut face rule on lattice cells (geometry §8.3), and a tilted wall crosses them obliquely | M-4.9's A-17 measures it against a lattice-aligned control; a dependence is S8b's to remove (M-4.3), never the fixture's to avoid |
| A-27 | Each value of the thin and far-field options (R-E6) produces a valid mesh on every case, not only the defaults | **[P]** `thin_gap: resolve` and `far_field: uncapped` only move the ladder the rest of the pipeline already handles; **[U]** `thin_material: template` collapses material, which no acceptance case has ever done, so it rests on M-4.3 and M-6.7 | M-4.6 runs every value on the fixtures it concerns; a value that cannot yet run is refused at parse time with the subtask that will enable it named, never left to fail inside the mesher |

## 11. Subtask rollup

| ID | Subtask | Tier | Assignable models | Multimodal | Status (2026-09-28) |
|---|---|---|---|---|---|
| M-0.1 | As-built spec revision (geometry 1.6, numerics 1.3, contracts 1.3) — text half landed 2026-09-23 | T3 / T2 | Kimi K3 Max / GPT 5.6 Sol Xhigh; GLM 5.2 Max / GPT 5.6 Sol Medium | no | ◐ text half landed 2026-09-23; geometry rev 1.7 (M-1.7) recorded |
| M-0.2 | Post-M-2/M-5 revision (geometry 1.8, numerics 1.4, contracts 1.4) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ waits on M-2/M-5 |
| M-1.0 | Verifier trust: MG-01 cap, MG-03 domain at stage 11, MG-08 contract validator, `[V13]` FAIL | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ● landed 2026-09-26 |
| M-1.1 | Matrix harness: both paths, logs kept, timing, census extended | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ◐ harness, both paths, logs, timing, `--focus` landed; `mesh-verify` now parallel and byte-identical (reference case 3 939 s -> 69 s, a8 51 s -> 16 s, 2026-09-29), still overlapped with the next case's meshing (`--jobs`); print-only S8 census parallel (a8 S8 166 -> 146 s) |
| M-1.2 | Byte-identity on the matrix | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ● all twelve cases, both paths, byte-identical at 1 thread vs default: nine via `check_determinism.py` (`determinism_{gated,default}.txt`), three reference cases in `determinism_reference.txt` (`1fa74df`) |
| M-1.3 | Time budgets | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ● budget line recorded (`b9aefe5`, host stated): gated a3 16.6 s, a4 83.8 s, a8 125.1 s; reference cases pending |
| M-1.4 | P2 baseline at equal fidelity | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ● table + sweep committed: default meets gated fidelity only on a1 (3.8× the tets) and a7a (4.7×); elsewhere never, within the leaf budget / 2 GB |
| M-1.5 | The P3 criterion: containment in the effective surface's facets (MG-02) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ● landed 2026-09-26 (`cont%`/`cover%`) |
| M-1.6 | Conditioning and identity: the one-ULP pairs under MG-11's constraints (was M-4.1) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ◐ S2 keeps constructed points on their facets (gated a3 98.51 %, a6a −37 % tets); `[V2]` PASS all nine both paths; move/merge report and default-path a3 regression open |
| M-1.7 | The X-pattern lattice: the 5-tet parity checkerboard, Rule T5, geometry rev 1.7 (R-E5, D-9 decided, S-53) | T2 / T3 | GLM 5.2 Max / GPT 5.6 Sol Medium; Kimi K3 Max / GPT 5.6 Sol Xhigh | **yes** — box-face wireframes beside the reference tool's own mesh, transition cutaways | ● landed `17e5a9d`; cost measured 2026-09-27 (gated a1/a3/a8 on-surface fell), a1/a3 repaid by `a718767`; open: a8 −0.69, a7b −0.2, a6b −0.06 pt gated; default-path `[V9]` a3 closed (`pierces_shared_by_an_edge`, `[V2]` 17 → 0), a6a/a6b open (whole-cell fan apex on the limb edge; M-2.3) |
| M-1.8 | Resolution as background × maximum level, given; per-input levels; the reference cases at their own resolution (R-E4, S-54, §6.2 item 18) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** (R9) — M-1.9's set, nine cases unchanged; reference-case contacts before/after | ● ladder, per-input levels and the reference cases on both paths at their own resolution (gated 0.96× / 0.74× / 1.34× the reference's tets) |
| M-1.9 | Focus-region render harness: intersection curves, sharp edges/corners, thin regions, transitions, from the input; before/after diffs (R9) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — the contact sheets, and the two known defects they must show | ◐ input-vs-output comparison harness landed (R9); thin-region and transition views, runner call open |
| M-2.0 | J1 fingerprint mismatch is a hard error; fingerprint = constraint entity ids (MG-06) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ● landed `9b9c1a2` |
| M-2.1 | Facet recovery by Steiner points on the constraint — gate | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | **yes** — cutaway renders at a3's cube edges and an a8 strut junction, before/after | ◐ 2026-09-29: the facet-split fan's non-star refusal answered by a kernel apex - gated a1 **100.000** and a7b **100.000** (`[V13]` PASS), a8 99.942, a4 99.971, a3 99.716, reference cases 99.597 / 99.789 / 99.939 %; whole-cell fans 0 on a1/a2/a7a/a7b, and after D-12 (one piece where no cap is left) a3 23, a4 11, a6a 4, a6b 1, a8 96 (earlier: move (d) on D-11 (a), +1–8 % tets, slivers for M-5) |
| M-2.2 | Boundary consistency: facet vertices on the cell boundary are trace points | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ● landed 2026-09-29: a rim vertex on a cell edge adopts the edge's K2 representative; hull classes 0 on a3/a4/a8; a4 99.911, a8 99.833, ref 3 99.854 |
| M-2.3 | Delete the whole-cell fan | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ blocked by M-2.1 on the reference cases (2 / 10 / 5 whole-cell fans); 0 on all nine acceptance cases since 2026-09-30; the deletion is written and tested (`data/output/patches/m23_delete_whole_cell_fan.patch`, nine cases byte-identical) |
| M-2.4 | Retire `contact_chamfered_by` | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ◐ counted (a6a 895, a6b 2,396 gated); blocked: the limb's contact strip is thinner than a lattice face, so the faces it declares are genuine rim chamfer |
| M-2.5 | P3 residual audit | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — the same cutaways | ○ |
| M-3.1 | The control | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ● all twelve cases, re-held 2026-09-29 after every M-2 step: gated FAIL sets equal the default's, gated `[V13]` better everywhere (gated a1/a2/a7b 100.000 PASS, a4 100.000, a3 99.927, a8 99.971; reference cases 99.936 / 99.935 / 99.958 % against the default's 95.248 / 96.097 / 96.831 %) |
| M-3.2 | Delete the five handles | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ○ |
| M-3.3 | Refactor `cut_lattice`, tests for the trace and the junction module | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-4.0a | One X per connected closed component; the source map (MG-04) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ |
| M-4.0b | Closure per component, no borrowed faces (MG-05) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-4.2 | T-junction loop: retire or trace upstream | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-4.3 | Thin fixtures exercise S8b; `[V7]` non-vacuous | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | **yes** — limb and gap cross-sections | ○ |
| M-4.4 | P1 campaign: A-3-ranked, A-4b, A-5, A-9, A-11 open sheet, A-12 non-cubic domain, A-13 defective input, real datasets, placement sweep | T2 (T1 runs) | GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — one render per new fixture | ○ |
| M-4.5 | Input decimation as S0 conditioning (D-1) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-4.6 | Resolution derived: the three `auto` combinations; D-10's options `thin_material`, `thin_gap`, `far_field`, every value; `curve_cells` re-measured; the fractions retired (G1, R-E4, R-E6) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ |
| M-4.7 | A-10 forging fixture and the G4 gate: band, sheet and marked contact in one mesh | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | **yes** — cross-section through the contact centre | ○ |
| M-4.8 | `FaceTagOrientation` per member; one geometric side per contact face (MG-07) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ◐ `FaceTagOrientation` per member landed (`3198276`); one geometric side per contact face open |
| M-4.9 | The thin family A-17..A-20: tilted, irregular and non-uniform thin plates and gaps, orientation invariance (R-B3) | T2 / T3 | GLM 5.2 Max / GPT 5.6 Sol Medium; Kimi K3 Max / GPT 5.6 Sol Xhigh | **yes** — cross-sections of every fixture, A-17's five orientations side by side | ○ |
| M-5.1 | Quality census by arm and dimension; G4-3 post-snap | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ — inherits D-11's cost (move (d): +1–8 % tets, a7a/a7b +28 %, more slivers) and D-12/D-13's sub-`eps` misattributed slivers (a4 5, a8 26 cells) |
| M-5.2 | Quality face triangulation in the J1 cache — gate | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ |
| M-5.3 | Near-edge trace points, constrained to exact surface points (MG-11) | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ |
| M-5.4 | S9 interior improvement under P3 | T3 | Kimi K3 Max / GPT 5.6 Sol Xhigh | no | ○ |
| M-6.1 | S10 as a stage; the partition flood fill built; no `s10_regions` | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-6.2 | The box as an S8 constraint, S11 trim before quality, INP, `[V10]`; `mesh` exits 0 (MG-13) | T3 / T2 | Kimi K3 Max / GPT 5.6 Sol Xhigh; GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-6.3 | `[V11]` compare mode | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-6.4 | Validation on exported meshes (GT-1..4, GT-6) | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — cross-sections and reference side-by-sides | ○ |
| M-6.5 | Performance envelope (GT-5) | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ○ |
| M-6.6 | `output.interface: welded \| split` — the user's interface export (D-4), `split` as node-star sectoring (MG-14) | T3 / T2 | Kimi K3 Max / GPT 5.6 Sol Xhigh; GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — Feature Edges of the split export | ○ |
| M-6.7 | `output.interface: cohesive` — sub-`t_sheet` gaps as a cohesive layer on both walls (D-8, §12.1); `SheetPairOffset`/`SheetThickness` retained at S8b | T3 / T2 | Kimi K3 Max / GPT 5.6 Sol Xhigh; GLM 5.2 Max / GPT 5.6 Sol Medium | **yes** — cross-section through the gap | ○ |
| M-7 | Certificate G1 repaired (MG-09), then GK-1, GK-2, GK-3, GG-1 as B.10 | T3 / T2 | Kimi K3 Max / GPT 5.6 Sol Xhigh; GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-8.1 | English docs incl. the S8 algorithm page and `AGENTS.md` | T2 | GLM 5.2 Max / GPT 5.6 Sol Medium | no | ○ |
| M-8.2 | Chinese mirror | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ○ |
| M-8.3 | This document's upkeep; the review file retired | T1 | GPT 5.6 Luna Max / DeepSeek V4 Pro Max | no | ○ |

Status legend: ● landed and accepted, ◐ partly landed (what is open is named), ○ not started.
Outside the rollup: **MG-15** (overlapping fan tets) fixed `b61c1fc`; `[V3]` folded faces 0 on
all nine, both paths.

Multimodal capability is required only for M-1.7, M-2.1, M-2.5, M-4.3, M-4.4, M-4.7, M-4.9,
M-6.4, M-6.6 and M-6.7; every other subtask is completable from text, documentation, logs and
source. T3 subtasks (M-0, M-1.5, M-1.6, M-1.7's spec revision, M-2.1, M-2.2, M-4.0a, M-4.3, M-4.6,
M-4.7, M-4.9's invariance diagnosis, M-5.2–M-5.4, M-6.2's box cut, M-6.6's sectoring, M-7's
certificate) are the semantic critical path
and should stay with as few distinct implementers as possible; M-1.0, M-1.8, M-4.4, M-4.5, M-6.x's
export halves and M-8.x are parallelisable against the frozen specs.

## 12. Decisions the owner must make

Questions this plan cannot settle by measurement, each with the consequence of either answer.
D-1..D-5 were decided on 2026-09-01 and the plan above reflects them; D-6..D-8 were opened on
2026-09-23 by the review's MG-10 (§1.3) and are **open**. They gate the *definition* three gates
are read against, not any subtask's start; the recommended answer is what the plan is written to.
D-9 and D-10 were opened on 2026-09-25 with the owner's requirements of that day (R-E4, R-E5)
and answered the same day: D-9 — which lattice M-1.7 builds — the 5-tet parity checkerboard; D-10
— what M-4.6's derived resolution chases — neither answer alone: both policies become user
options, the recommended answers their defaults, every option built and tested (R-E6).

| # | decision | if yes | if no | decided |
|---|---|---|---|---|
| D-1 | **May S0 decimate a finely tessellated input within a stated tolerance before meshing?** The reference cases carry their surface at or below `h`, and an exact-conforming mesh must carry every vertex of it (§2.3) | a repair option `decimate: <tol>` becomes part of the *input conditioning* the user chooses for their geometry — P3 stays exact against the conditioned input, and the reference-case counts come down toward the shipped path's; not a mesher knob, because it changes the input, not the cut | the element count on real geometry is what it is: bounded below by the input's vertex density; P2 is judged at equal fidelity only (M-1.4) | **yes** → M-4.5 |
| D-2 | **Is the record committed?** The 10,490-line record was git-excluded and lived on one disk | it is committed as `RECORD_mesh_generation.md` | it is not committed | **no**, then **deleted** on the owner's instruction (2026-09-01); Appendix A carries every finding this plan cites, Appendix B every design it needs |
| D-3 | **May the lattice be aligned with the input?** a2 meshes exactly and cheaply *because* its faces coincide with lattice planes; the same coincidence is what produces the one-ULP `[V2]` pairs on a3 and a8 | M-4.1 uses input quantisation only and keeps alignment; a2's count stands | M-4.1's lattice offset makes a2 an ordinary cut case (+elements, still exact) and removes the coincidence class everywhere | **yes** → M-4.1 rule (a) only |
| D-4 | **Does "a marked interface layer, similar to a crack/interface layer embedded in the mesh" (G4) require split nodes?** The record's taxonomy makes a marked interface *welded* — C0, one node set, `(elem⁺, elem⁻)` per face so a solver inserts the cohesive or contact treatment — and reserves split-node meshing as out of scope for iteration 1 | M-6.6 exports the split at S11 (duplicated node sets along the tagged interface, INP only; the VTU stays welded) | the welded interface with side-element pairs is the deliverable | **neither — the user chooses per run**: `output.interface: welded \| split`, default `welded`, validated and documented → M-6.6; R7 exempts exactly the pairs `split` creates |
| D-5 | **Is the reference-verbatim SAMR fallback (assumption G-5: 5-tet checkerboard + 7/12-child SAMR + hanging closure) still wanted?** It was retained for gate G4-3, which never failed, and has never been needed | it stays in the design and M-5.1 measures G4-3 at last, so the fallback has a trigger | it is struck from the record's design (M-0.1) and the octree-plus-templates lattice is the only lattice | **yes** → stays; M-5.1 measures G4-3. (Rev 3.3: D-9 makes the 5-tet checkerboard the primary lattice (M-1.7), which also lifts the fans' pre-snap worst from 35.264° to 45°; the fallback keeps only its 7/12-child refinement and hanging-node closure) |
| D-6 | **Which surface does P3 hold against** — the original input, the effective surface (after S0 repair/decimation and S2's `ε`-merge), or the collapsed model (after S8b)? §1.3 | *recommended:* the **effective** surface for the gate; the original-vs-effective displacement reported per run (M-4.5 already says so for decimation; repair and `ε`-merge join it); the collapsed model's departure from the effective surface reported as its own `[V7]` metric, never folded into `[V13]` | P3 against the original: decimation (D-1) and `ε`-merging become P3 violations and M-4.5 is withdrawn | **open** (2026-09-23) |
| D-7 | **Input-forced low quality** — a wedge whose material angle is below `[V4]`'s dihedral gate cannot meet the gate (§1.3). Per-element exception with report, a per-element gate from the local input angle, or a declared failure naming the wedge? | *recommended:* the exception — `[V4]` counts input-forced elements separately, each with the forcing angle, and PASSes the gate on the rest; a solver-facing report lists them | a declared failure: any input with a sharp material angle is unmeshable by definition, which contradicts P1 | **open** |
| D-8 | **Welded-sheet semantics under P3** — is the collapsed sheet the accepted representation of a sub-`t_sheet` gap, must such a gap be kept two-sided (contact with `split`, D-4), or is it delivered as a **cohesive layer** whose two faces sit on the two original walls (§12.1)? | *directed (2026-09-24):* the cohesive layer — the collapse stays as the meshing device, S8b retains the wall points, `output.interface: cohesive` un-collapses them at export (M-6.7); P3 then holds on both walls against the effective surface (D-6) and "the bodies never weld" (B.9, A-7a) holds literally on `cohesive` and `split`, "never weld *silently*" on `welded` | collapsed-only or two-sided-only: either the gap's geometry or R-B1's sheet regime is given up | **directed** → M-6.7; sub-decisions D-8a..D-8c open (§12.1) |
| D-9 | **Is R-E5's X pattern delivered by the parity-reflected Freudenthal lattice?** The alternative is the reference tool's own 5-tet parity checkerboard. Both show the X on every axis plane, both conform under §3's transition machinery on every balanced configuration Appendix D.1 builds, and both raise the transition fans' worst dihedral from 35.264° to 45° | *recommended:* **yes** — the smallest change that delivers the pattern: every tet congruent to today's, the same element count, the same plain-cell quality every S7/S8 measurement in §2 was taken on; one corner relabelling and one face rule in `lattice.rs` (M-1.7 as written) | the **5-tet checkerboard**: plain leaves emit five tets instead of six (1,080 against 1,296 on a uniform 6³ block), at a better plain-cell worst (54.736°, AR 1.3660); every plain-cell cut population changes shape (a regular central tet of twice the corner tets' volume, no body diagonal), so M-1.7 adds an equal-fidelity comparison before it lands; D-5's fallback keeps only its refinement and closure | **no — the 5-tet checkerboard** (owner, 2026-09-25) → M-1.7 |
| D-10 | **What does derived resolution chase (M-4.6)?** (a) *Thin regions:* a region thinner than the shape-driven field resolves can be refined until it is volumetric, or handed to the band/sheet path at that field. (b) *The far field:* where no request binds, the background edge is the one scale the geometry cannot supply | *recommended:* (a) **by kind** — a thin *material* region (`intra(X)`) is refined at least until it leaves the sheet regime and never collapses under `auto`, because a collapsed limb puts its boundary on neither wall and D-8b (the material wedge) is not adopted; a thin *void* gap (`inter`) below the shape field's resolution goes to the band/sheet path, which D-8's cohesive export delivers on both walls. That is the cheapest representation `auto` can choose that stays exact under D-6 and D-8. (b) **today's far field**, `0.05` of the domain diagonal fitted to the domain, because every number in §2 was measured with it and a solver's far field should not change unasked; a user who wants another states the background | (a) *chase everything*: every thin region refined to `gap_cells` across down to the envelope floor — a forging contact drives the level to that floor and the thin path never fires under `auto`; or *template everything*: A-6's and A-8's thin material collapses under `auto`. (b) *pure P2*: the coarsest background the domain admits, the graded field alone shaping the far field (at `grading = 2` the requested edge grows by one unit per unit of distance) | **both, as user options** (owner, 2026-09-25): `sizing.thin_material: resolve \| template` (default `resolve`), `sizing.thin_gap: template \| resolve` (default `template`), `sizing.far_field: capped \| uncapped` (default `capped`); every value built and tested → M-4.6 (e), R-E6. If D-8b later adopts the material wedge, `thin_material: template` exports a collapsed limb on both walls as well |
| D-11 | **Take P3 at a P2/P4 cost?** Move (d) (M-2.1 status, 2026-09-28) lifts gated on-surface on every case (a8 99.12 → 99.78 %, a3 98.91 → 99.64 %, reference case 1 95.73 → 97.70 %) and raises tets 1–8 % (a7a/a7b +28 %) and the share of tets below the dihedral floor (a8 4.1 → 5.4 %) | land it now and recover P2/P4 later (M-5) | first make it cheaper (fewer points, placement that avoids slivers), land only if strictly better | **(a)**, 2026-09-28: geometry first; AR/quality is M-5's job |
| D-12 | **One piece where no cap is left?** When every surface triangle in a declined cell lies in the cell's own faces, the facet-split fan refused ("no surface separates the cell") and the cell took the whole-cell fan. Taking it as one piece with one label: whole-cell fans a8 630 → 96, a3 106 → 23, tets a8 −4,107 / a3 −754, P3 and every status unchanged; a8 gains one misattributed cell (5e-11), a sliver thinner than `eps` where the rim was matched onto face nodes | land it | first make the envelope sliver label correctly | **(a)**, 2026-09-29 |
| D-13 | **Move an on-surface boundary triangle to the piece it closes?** A boundary triangle whose corners are all on the surface is placed by a centroid that is itself on the surface; placed wrong, one piece is open and the cell falls to the whole-cell fan. Moving it where both pieces close: a4 whole-cell fans 11 → 0 and on-surface 99.971 → 100.000 %, a3 99.716 → 99.850 %, a8 fans 96 → 39; cost 4 (a4, 2.2e-12) and 53 (a8, 1.1e-10) misattributed micro-slivers inside `eps`, a4 `[V5]` PASS → WARN | land it | first label the sub-`eps` slivers | **(a)**, 2026-09-29, and **as a standing rule**: a change whose only cost is sub-`eps` slivers mislabelled, everything else unchanged or better, is landed without asking and its numbers recorded here |

### 12.1 Study — a cohesive layer for gaps below `t_sheet` (D-8, 2026-09-24)

**The question.** A void gap between two bodies narrower than `t_sheet = τ_s·h` (`τ_s = 0.2`
today, numerics §11) has three candidate representations, and the two the plan carried each give
up one goal:

| representation | what the solver receives | P3 against the walls | what is lost |
|---|---|---|---|
| collapsed sheet (R-B1 as frozen; geometry §8.1) | one node set, C0 across the gap, `FaceTagSideElems` per sheet face | on **neither** wall; the boundary sits at the midpoint, up to `t_sheet/2` off each | the gap's volume, and "two bodies with a void between" becomes one bonded body the solver was never told about |
| two-sided contact (`split`, D-4, M-6.6) | one copy per sector, the copies coincident at the midpoint, nothing between them | still off both walls by the same amount | the gap; the contact pair and its law are the user's to add by hand |
| **cohesive layer** (this study) | the same copies **placed on their own walls**, one 6-node wedge per sheet face between them | on both walls exactly, at the very points the collapse measured | nothing geometric; the layer's constitutive law is the user's, as a material always is |

**Why it fits what is already built.** Three pieces exist and one is missing.

1. G7-2's collapse is *per vertex pair*: a collapsed cut node is the midpoint of two wall
   crossings (geometry §8.1, "the rim node is the midpoint of the two crossings"), so the two wall
   points are known at the moment of collapse. They are discarded today — that is the whole gap
   (S-52).
2. `FaceTagSideElems` (elem⁺, elem⁻) is exactly the adjacency a wedge needs, and contracts §2.3
   reserves it "for cohesive/split-node treatment" (B.6). Nothing has ever consumed it.
3. M-6.6's node-star sectoring already yields one copy per connected sector, so `cohesive` is
   `split` plus a displacement per copy and a wedge per face — not a third meshing mode.

**The mechanism**, S8b to S11, is specified in M-6.7. The points that needed checking:

- *Validity of the un-collapse.* Moving a copy to its wall moves a vertex of every tet in that
  sector by at most `t_sheet/2 = 0.1·h`. S7 already moves nodes up to `0.30·l_min` under an exact
  no-inversion test; the same test decides each move, and a move that would invert a tet is
  declined per node, reported, and the wedge carries the measured thickness as a section property
  instead (D-8a). No setting chooses between the two (R3).
- *The rim.* A collapsed node next to an uncollapsed one is a rim node; the band layer beyond it
  already has distinct nodes on each wall, and sectoring puts each band tet in the sector of the
  wall it touches, so the copies land where the band's wall nodes are and the wedge tapers into
  the band with no new rule. `[V3]` across the rim is the acceptance.
- *The contact end.* Where the gap closes (`t ≤ ε`, A-10's centre), the wedge's thickness goes to
  zero continuously and the element is D-4's split contact with a zero-thickness cohesive element;
  where three bodies meet on a rim, M-6.6's sectoring rule (one copy per sector) already applies.
- *The verifier.* A wedge is not a tet: `[V4]` excludes it by type (a cohesive element's thickness
  is constitutive, not a dihedral), `[V7]` gains the thickness clause, `[V10]` checks the wedges
  through M-6.6's map, and `[V5]` reconciles `Σ area·t` against the input's gap volume (A-7a's is
  analytic). Each clause has its negative fixture before it is trusted (R8).

**What this settles.** MG-10's second conflict dissolves: P3 holds on both walls against the
effective surface (D-6's recommended answer), the collapse is a meshing device rather than the
delivered geometry, and B.9's "the bodies never weld" holds literally. `collapse_sheets` gains a
delivered meaning at every export and stops being a knob (M-4.3).

**What stays open.**

- **D-8a** — a copy that cannot reach its wall: accept the specified-thickness wedge with a
  report (*recommended*), or refine the cell (K1's remedy) until it can?
- **D-8b** — the thin *material* limb (`intra(X)`, A-6a) is the mirror case: the same wedge with
  a continuum response carrying material X. Not adopted here — a thin material limb also has a
  solid-shell representation that a cohesive element does not replace — and left to the owner.
- **D-8c** — the default of `output.interface` for a run whose mesh contains a sheet: `welded`
  (today's default; the gap delivered bonded) or `cohesive` (*recommended* — a bonded joint the
  user did not ask for is a silent change of the problem, a cohesive layer with a user-supplied law
  is not).
- Not studied: `fem_profile`s without cohesive elements (they take `split`), and a gap that is not
  empty (a thin third body between two others is three components and a band, never a sheet).

## 13. Reproduction — every number in this document

| what | command | notes |
|---|---|---|
| §2.2 default column | `python3 data/fixtures/meshgen/acceptance/run_acceptance.py --json default.json` | at `0a8eb1c`, 2026-09-01; per-case verify JSON under `data/output/acceptance/<case>.json` |
| §2.2 gated column | `RUSTMSPT_PLC_PASS=1 RUSTMSPT_PLC_DIAG=1 python3 data/fixtures/meshgen/acceptance/run_acceptance.py --json gated.json` | same commit; the runner overwrites the default run's per-case JSON — M-1.1 fixes that |
| §2.3 reference, both paths | `python3 data/fixtures/meshgen/acceptance/run_reference.py` with and without `RUSTMSPT_PLC_PASS=1` | `RUSTMSPT_REFERENCE_DATASET` at the dataset root; `data/output/reference/summary.json` |
| §2.4 by-arm tables | `RUSTMSPT_PLC_PASS=1 RUSTMSPT_CUT_DIAG=1 RUSTMSPT_PLC_DIAG=1 rustmspt mesh --config <case>.yaml` and read the `[PLC]` lines; `plc_path` in the contract VTU | the lines named in §2.4 |
| §2.6 byte-identity | `RAYON_NUM_THREADS={1,8} RUSTMSPT_PLC_PASS=1 rustmspt mesh --config a1.yaml; sha256sum <name>_s08_cut*.vtu` | a1 only today; M-1.2 does the matrix |
| §2.1 test count | `cargo test --release` | 210 passed, 10 suites, 0 failed |
| §2.1 handles | `grep -rhoE 'RUSTMSPT_[A-Z0-9_]+' src \| sort \| uniq -c` | 29 unique names at `891badc`: five behaviour-changing, two shared device/backend selectors, seven build-identity, the rest print/dump-only (`RUSTMSPT_CUT_DIAG` also adds three cell arrays to the written `s08`) |
| §2.2 thin-path finding | `RUSTMSPT_THIN_DIAG=1 rustmspt mesh --config a7a.yaml` (and a6a) and read the `[S3/G3-2]`, `[THIN-SKIP]`, `[S8b/G7-1]` and `[S8b-DIAG]` lines | default path |
| §2.6 timings | `RUSTMSPT_TIME_STAGES=1` on the §2.4 runs; `[STAGE-TIME]` lines on stderr | under concurrent load |
| §2.7 curve carriage, `[V9]` clauses | `[V9]` metrics `curves_declared`, `curves_carried`, `curve_edges`, `radial_mismatches`, `open_junction_fans` in each case's verify JSON, both paths | same runs as §2.2 |
| §2.7 G2 gaps S-12/S-13 | `grep -nE "sharp_corner\|feature_curve\|curve_conformance" src/meshgen/verify.rs` (empty); `grep -rn override src/meshgen/features.rs src/config/meshgen.rs` (doc comment only) | at `0a8eb1c` |
| §2.7 per-case sizing overrides | `data/fixtures/meshgen/acceptance/run_acceptance.py` `CASES` and the comment above it | A-4, A-6, A-8 |
| §2.1 / §6.5 review counterexamples | `python3 reproduce.py <repo>` (Appendix C; `BASE` pointed at a fresh directory) | re-run 2026-09-23 at `891badc`, `git_dirty = true` on non-meshgen files (`rustmspt version --json`); results in that run's `results.json` |
| §6.5 MG-09 arithmetic | the `gpu_g1` block of Appendix C (`struct`-based f32 rounding in G1's expansion order; `fractions.Fraction` for the exact f64 determinant) | stand-alone; no GPU needed |
| §6.5 MG-11 arithmetic | `python3 -c "import math; q=1e-5; x=0.5/math.sqrt(3); print(round(x/q)*q*math.sqrt(3))"` → `0.5000084271289835` | — |
| spec audit, 2026-09-23 | the audit read every MUST/table/constant/log tag of the three freezes against `src/meshgen/`, `src/io/vtu.rs`, `src/pipeline/{meshgen,mesh_verify}.rs` and `tests/`; 43 geometry findings were then read again by one independent verifier (42 confirmed, 1 refuted, 2 re-labelled) before the pass was stopped on cost, and every other finding is single-pass, spot-checked by grep; the outcome is each spec's verification record (geometry §14 [11], numerics §10 [A1], contracts §8) | commands per finding are in those records |
| §2.1 test count, 2026-09-23 | `cargo test --release --offline` | see §2.1 |
| §2.1 env-var census, 2026-09-23 | `grep -rhoE 'RUSTMSPT_[A-Z0-9_]+' src \| sort \| uniq -c` and the read site of each | the class table in §2.1 |
| geometry §10 as-built (D-40..D-43) | `cargo test --offline --release --test meshgen_arrange_tests` (48 passed); `rustmspt mesh` with `data/fixtures/meshgen/acceptance/a2_cube.stl` listed twice at priority 0 under `coincidence: warn` (read the `[G2-4]` and `[S6/G5-1]` lines); the five patch-level probes ran in an uncommitted scratch crate against the library | the probes become fixtures A-15/A-16 (M-4.0) |
| contracts §1/§2 as-built (D-18..D-21) | A-6a and A-3 `s08` runs from `run_acceptance.py`'s config template (`snapshots: all` / `key`), arrays read with an ascii regex parser; `mesh-verify` on A-3's delivered and contract files | — |
| §2.3 correction, §6.2 item 18 (rev 3.3) | `python3 reference_resolution.py "$RUSTMSPT_REFERENCE_DATASET"` (Appendix D.2) | reads each reference case's `*RVE_CORNERS`, `*NUM_ELEMENT`, per-section `*SAMR_LEVEL` and log; prints the reference's background and levels beside the ladder `run_reference.py`'s mapping realises here; stdlib only |
| M-1.7's lattice rows, S-53 (rev 3.3) | `python3 lattice_pattern.py` (Appendix D.1) | stdlib only, exact integer and rational arithmetic, ~2 s; reproduces geometry §3.7's frozen Rule-D rows to every printed digit as its own check, then Rule X and Rule T5; conformity on seven balanced configurations plus the negative control; the uniform-block direction and valence counts |
| numerics §6/§7/§9 as-built (D-23..D-26) | `grep -rn 'meshgen\|gwn\|winding\|orient' src/gpu/`; `grep -rn gwn_margin_band src tests`; `grep -rn 'two_sum\|two_prod' src tests`; `cargo test --offline --release --lib meshgen::topo` (4 passed) | — |

---

# Appendix A — the record, condensed

The previous `PLAN_mesh_generation.md` was deleted on 2026-09-01. Every entry below is a section
this plan cites, keyed by that file's section number, with the finding and the number the plan
relies on. Each was measured with the scripts in §13 or their predecessors at the date given;
where a number has since been re-measured, §2 is authoritative. A key listing several sections
(`§6.62 / §6.63`, `§10.1 / … / §10.6–§10.7`) resolves every one of them. `AGENTS.md`'s meshgen lessons
(its P-3.13 section and the G6-0..G7 lessons) are not reproduced — that file stays.

| key | date | finding, and the number |
|---|---|---|
| §0.1 | 2026-08-06 | The S5 lattice alone verifies `[V3]`-clean on 1.65 M tets of real geometry; 335 tests then (343 with `gpu`). S0–S8 complete; S9–S11 pending. |
| §1 | rev 2 | The requirements register R-A1..R-N1, "condensed from rev 1" — the original goals (§4 of this plan carries it verbatim). Out of scope for iteration 1: split-node sheets (export contract reserved), Tet10, periodic BCs, MPI, out-of-core. |
| §2.0 | 2026-08-15 | The recorded "P2 FAIL, 3.6× (1,269,546 vs 354,372)" did not reproduce and was never a matched comparison. At the reference tool's own resolution (parsed from its logs) this mesher emitted **0.40× / 0.47× / 0.41×** (142,645/354,372; 341,380/730,199; 477,911/1,167,239) on the shipped path. Superseded by §2.3. |
| §2.2 | 2026-08-15 | The fan's cost, from `[V12]`'s emission rates on the S5 lattice: untouched cell 1.00 tets, §6 table cut 3.8–4.6, escalated fan **10.1–20.5**; the off-surface share tracked the fanned share case by case (a7a 26.6 % off / 22.4 % fanned … a2 0 / 0). |
| §6.1 | 2026-08-15 | Escalation census over nine cases: `junction` 6,785 cells carried **85.7 %** of off-surface area (72.4 % of its own area off); `multi_crossing` 2,068 cells, 2.7 %; no-escalation 11.5 % (later measured: raw-lattice-face damage only 0.3 %); `dry_run` and `quality` never fire. |
| §6.2 | 2026-08-15 | The centroid fan appears nowhere in the frozen spec: §7.1–§7.5 specify a local PLC mesher, §7.4 answers the determinism objection in four bullets, §7.6's fallback is curve-pinned kirigami. "P-3 is implement §7 as frozen." The `meets_inside` gate was deleted the same day (P-3.2 part 1: suite off-surface 1.310 → 0.964). |
| §6.4 | 2026-08-15 | After the gate deletion ~75 % of remaining damage was crease damage (a6b 93.6 %, a7b 90.4 %, a8 89.5 %, a6a 88.9 % of their own). S7 curve snapping refuted: 24 of 1,404 segments covered, edge-aware 53 with P3 slightly worse, cap 0.49 → 149; laying a lattice edge on an arbitrary line needs up to 0.707 `l_min`. The `split_escalated_cell` reuse refuted: 748 "chord lies along a walk edge". |
| §6.6, §6.18 | 2026-08-16 | Widening the crease claim to neighbours (a `Crease` escalation reason, face-first pre-pass): both variants worse; re-tested at the finer resolution: a8 +0.177, a6a +0.146, a3 −0.325 for +1.1–1.3 % elements — net ≈ 0, refuted twice. |
| §6.9 | 2026-08-16 | Method: `node_origin` (0 lattice / 1 interned) and `constraint_kind` charge each off-surface face to the kind of its worst corner. a8 then: 76.0 % on a free lattice node, 24.0 % on an interned node, 0 % bound, 0 % never-cut. |
| §6.10 | 2026-08-16 | `gap_cells` 2 and 3 identical to the digit; **4** takes a7a 88.717 → 99.829 % and a7b 91.377 → 99.892 % (2.75× / 3.64× elements). Below four no lattice vertex lands in the gap, so `gap_cells < 4` chooses an incorrect mesh — the floor moved to 4 (R3). Suite off-surface 0.8885 → 0.4872. |
| §6.16 | 2026-08-16 | Refinement is exhausted: the sizing field sits at `h_min` on every case that refines (a3/a4/a7a 0.020785, a6a 0.006928, a8 0.010392); routing the sub-cell body through K1's refine channel fires (448 cells, 3 passes, +4,221 leaves) and moves a8 92.011 → 91.980 % for +1.8 %; a8 reports 2,269 escalations still open after 3 passes; lowering the floor to 0.003 breaks `[V1]`/`[V3]`. Only the fragment cut remains. |
| §6.31 | 2026-08-20 | Conforming face triangulation (split each constraint until the plain Delaunay contains it): a6a 4 s → over 10 minutes, elements up for the same gain. Reverted whole. Fixable in principle; cost is the reason. |
| §6.42 | 2026-08-22 | Method, no code change: one CSV row per traced cell (`RUSTMSPT_PLC_CSV`, 36,256 rows a6a / 16,883 a3, pandas): the decline rate rises 0 % → 100 % with fragment size and is flat against cell shape. |
| §6.43 | 2026-08-22 | The facet's rim comes from the faces: a clipped surface triangle meets a face plane in a segment whose endpoints `trace_on_face` already interned, so the rim vertex is matched to that node. "Hull carries a node the boundary has never heard of" declines fell **96 %**. |
| §6.45 | 2026-08-22 | The gated path first compared on goal properties: 3.2× worse P3 on a6a, 2.0× on a3, +76 % elements on a1 — better near curves, worse away. Four steps of "§7.4 takes 98.8 % of traced cells" had improved a proxy while P3 degraded. `[V1]`'s inverted tets fixed at the source: a tet with volume below `longest² × tol` is declined (a6a 20 → 0, a3 10 → 0). Narrowing the triage to node-set inadequacy: 13× fewer cells offered, P3 **worse** (a6a away-from-curve 1.30e-2 → 1.57e-2). |
| §6.46 | 2026-08-22 | P3 charged to the emitting arm for the first time: on a6a §6's table 0.0 % off, §7.4-meshed 0.0 %, the declined arm all of it. |
| §6.49 | 2026-08-23 | a4's serrated lip: 720 faces, 3.889e-3 area, up to 4.3 % of `h` proud of the cube edge, all on free interned nodes in `Junction` tets — the fan at feature curves; on a4 100 % of off-surface area is in escalated cells. Facet-interior recovery by edge removal converts 14 of 1,849 cells; **90.4 %** of `remove_edge` refusals are "the link polygon has no valid triangulation". The Steiner cone in the link: built 687 of 692, converted **2** — a cone adds `n+2` edges through the region the facet crosses. `facets_cross` = 0 cells on a3 (the arrangement already resolves crossings). The change that mattered: split the boundary soup by the surface, cap both halves with the surface's own triangles, fan each piece — a6a's off-surface area **−89 %**. |
| §6.50 | 2026-08-23 | a1 by arm: §5.2 table 4.59 tets/cell, §7.4-meshed 16.90, facet-split fan 37.45; each traced cell is handed 20.08 boundary triangles from 12.04 trace points — the input sphere is tessellated at 0.045 against `h` 0.05. At `h/2` the premium falls +93 % → +24 %. **At equal fidelity**: gated 62,229 tets at 99.7885 % on-surface vs shipped-at-`h/2` 225,296 at 98.5190 %. Tetrahedralising each split piece (convex, no interior constraint) instead of fanning: a1 62,229 → 61,197, a3 281,737 → 276,539, a6a 353,102 → 351,795. |
| §6.53 | 2026-08-23 | a8 through the gate for the first time: on-surface 92.038 → 95.093 %, off-surface −38 %, +14.9 % tets — and `[V3]` **FAIL**: 2,268 leaks, 113,859 hanging nodes, 173 non-manifold edges, on a mechanism a1/a3/a6a cannot exhibit. Conformity is not a trade. |
| §6.54 | 2026-08-23 | On a1, tets below 10°: §5.2 table 5 (5 inherit a needle face), §7.4-meshed 5,046 (**4,756 = 94.3 %** inherit), facet-split fan 3,297 (2,730 = 82.8 %). The quality defect is two-dimensional first; interior Steiner points would reach 6 %. |
| §6.56 | 2026-08-23 | a8's whole conformity failure seeded by **3** cells owning a face that would not triangulate (a private 1e-9 relative bound, the point seven orders inside `eps`), spread to 46,596 excluded cells. Spreading the exclusion outward made it worse (122,551 excluded, 289,135 hanging). Instrument the seed, not the population. |
| §6.57 | 2026-08-23 | A T-junction detector agreeing with `[V3]` at `1e-9 × diagonal` (the nodes are ~1e-12 off the face, not on it); the repair replaces the flat sliver and its neighbour by three tets, all-or-nothing per node, with a duplicate guard, rim handled as an edge, and a fixed point: a8 58 → 1, a3 18 → 7 hanging nodes. Six wrong versions preceded it. |
| §6.60 | 2026-08-26 | The residual duplicate pairs are **2.98e-8 apart = one float32 ULP**: `f32(0.3217) + 0.15625` vs `f32(0.8217) − 0.34375`, both inputs clean (8 and 642 distinct vertices, nothing closer than 1e-6). Recomputing endpoints from the edge (`edge_crossing`) changed nothing (100 → 100 duplicates). |
| §6.61 | 2026-08-26 | The edge split with deletion, validated on a3 only, regressed a8 from **1 → 6** hanging incidences (off-surface 7.6599e-3 → 7.7078e-3). Reverted; control on a8 before claiming a change safe. |
| §6.62 / §6.63 | 2026-08-26 | For every trace endpoint the snap declines, the nearest candidate it can see is 1e-1..1e-3 away, never closer — publication, not tolerance. Publishing against the nearest edge and snapping at `quantum` collapses a3's traced cells **16,883 → 190**; the near-edge distances are a continuum (decades 1e-1..1e-5), so no merge rule closes them. |
| §6.64 | 2026-08-26 | The pairs share no tet, so they are not collapsible edges: unconditional merge 103 leaks / 513 non-manifold; link-validated collapse 15 non-manifold, 185 hanging. Cause: lattice edge (19670, 20199) is dyadic with `x − y = 0.5` exactly; the cube's feature edge has `x − y = 0.499999970198` — coplanar by construction, missing by one f32 ULP; both edge ends outside the cube so §5.2 interns no crossing. "A conditioning problem: where the lattice sits relative to the input." |
| §6.65 | 2026-08-26 | Zero hanging nodes on every case: the on-edge interning bound was a private `1e-9` relative test and the pairs sat at 6.7e-7 of the face's edge; using `[V2]`'s own `1e-6 × diagonal` (the config's `coincidence: merge`) closes it. a1 59,472 / a6a 351,482 / a3 271,929 / a8 1,603,275 tets; `[V3]` PASS on all; `[V2]` a3 100 → 2, a8 36 → 6. Cost: `[V6]` undeclared a3 7,688 → 8,469. |
| §6.66 | 2026-08-27 | `[V6]`: `[V13]` reads the boundary off region keys and counted 8,468 more boundary faces than declared on a3; every two-component step side was in a cell §7.4 refused. Five changes (declare the labelled boundary one component at a time; clip caps per piece; merge identical caps; `conform_cap_rim`; declare chamfered contact within the fan's own chamfer where S2 declares coincidence): a6a and a8 `[V6]` PASS, a3 199 → 61, a6a off-surface halved. Refuted: removing `on_cell_face` (`[V3]` breaks), the inner sample (a6a builds 304 → 279), the rim-run side in three forms (+731 tets, +2 duplicates, `[V9]` −6, `[V6]` unchanged). Every `[V9]` node is interned, on the curve, with `constraint_kind = Free`. |
| §9154 | 2026-08-14 | A-3's placement sweep: six placements of the whole scene; the shipped `x_min = 0.50` (on a lattice plane) is best — `[V6]` 80 against 71–123, `[V9]` missing 1 against 2–18, the only `[V5]` PASS (3.8 % vs 22.1 %), 10–25 % cheaper. Alignment with the input is a net advantage (D-3). |
| §9837 | 2026-08-14 | The spoke cut (cut fan tets on a plane through three crossings) reverted at `7b80b1f`, not disabled: it fired wherever it could (a7a 1,492 times) and left the plate 2.6× thinner-than-true; it moved `undeclared_boundary_faces` −26 % without moving the serration. It shipped behind `RUSTMSPT_NO_SPOKE_CUT` with a question about the default, and the question was the finding: a feature that needs a flag to decide whether it helps is not finished (R3). The `RUSTMSPT_SPOKE_PROBE` diagnostic stayed. |
| §10.1 / §10.2 / §10.3–§10.4 / §10.5 / §10.6–§10.7 | rev 2 | Stage designs S0–S5 as recorded, all implemented: S0 welding + `repair: strict \| conservative \| permissive` with a structured action log and the `s00` snapshot; S1 sharp edges by dihedral (45°), rims and non-manifold edges always features, polylines with corners (the "explicit point-list overrides" clause was never built — S-13); S2 corefinement with the global intersection registry (each intersection vertex constructed once from its defining entities), staged f64 → double-double, coplanar overlay, coincidence policy table, radial patch ordering; S2b topology rebuild with generalized-winding-number fallback for defective solids (sheets never claim volume); S3 gap field (two-sided rays + closest-pair sweep + densification + five-check pairing battery, confidence 0.9, hysteresis regimes); S4 sizing from curvature/chord, features, LFS, gaps, curves with the S3↔S4 monotone coupling (`h` a running minimum); S5 strongly 2:1-balanced Morton octree, Freudenthal 6-tet + centroid-fan transitions (SPEC geometry §2–§3), reference-verbatim 5-tet SAMR as the G4-3 fallback (kept, D-5). |
| §R | 2026-09-11 | The review's own verification: `cargo test --release --offline` on six suites (`mesh_verify`, `meshgen_surface`, `meshgen_config`, `meshgen_cut`, `meshgen_band`, `meshgen_quality_gate`) — **93 passed, 0 failed** (16/19/15/20/22/1), which shows the suite does not cover its counterexamples, not that the module passes acceptance; six verifier counterexamples and four small mesh scenes run through the release CLI, one numeric counterexample through `Fraction` plus step-wise f32; every mesh scene wrote its S0/S2/S8 snapshots and exited 1 at the known `S9..S11 NotAvailable`, none recorded as a delivery; the script repeated in a fresh directory gave the same results. Not done: the nine-plus-three matrix, GPU tests, a real FEM solve, and any proof that a given PLC recovery closes every input. MG-06 is static confirmation; the algorithms proposed under MG-10..MG-14 are not verified implementations. |

# Appendix B — designs carried from the record

Content the specs do not hold and the phases need. Text is the record's, lightly condensed; where
this plan changes it, the change is marked.

## B.0 Where the record's section numbers point now

The three freezes cite the rev-2 record by section number ("plan §10.3", "plan §12", …). The
record is deleted (D-2); this table resolves every such citation. A citation of the form
*record §N* in this document is an Appendix A row.

| cited as | content | lives now in |
|---|---|---|
| plan §2 (topic A, tooling-first) | the design review's conclusions: Freudenthal over the 5-tet checkerboard — **reversed by D-9 on 2026-09-25** (its stated reason does not hold, S-53); verifier before mesher | A-§2.0/§2.2 rows; geometry §2.3's stated reason; contracts §6 |
| plan §5.3–§5.5 | ID semantics: `N_ID`, partitions, worked examples | **B.1** |
| plan §7.2 / §7.3 | the VTU schema sketch and the writer precedent | contracts spec §1–§2 (the sketch is superseded by the freeze); the hand-rolled writer precedent is `src/io/vtu.rs` |
| plan §10.1 / §10.2 | S0 conditioning; S1 features | A-§10.1–§10.7 row; geometry §5.1 (on-cut vertices), numerics §2 rows S0/S1 |
| plan §10.3 | S2 intersection registry | numerics §5; geometry §7.2 |
| plan §10.4 | S2b topology, the sheets-never-claim-volume hard guard | geometry §9.1 row 10; numerics §2 rows S2b; M-4.0b |
| plan §10.5 | S3 gap field | geometry §11; A-§10.1–§10.7 |
| plan §10.6–§10.7 | S4 sizing, S5 lattice, the reference-verbatim SAMR fallback (D-5) | geometry §2–§3; A-§10.1–§10.7 |
| plan §10.11 | the FEM-aware ladder for band cells (outcomes 1–5) | geometry §4.4 and §12 ARB-20; `thin.rs` |
| plan §10.12 | S9 quality design | **B.5** |
| plan §10.13 | S10 regions, sheets taxonomy, welded semantics, `FaceTagSideElems` | **B.6** |
| plan §10.14 | S11 export, material mapping | **B.7** |
| plan §12 | the determinism contract (strict/fast) | **B.2**; numerics §8 |
| plan §13 | GPU/CPU split | **B.3** |
| plan §14 | scale and memory envelope; the snapshot size WARN; the pair-count guard | **B.4** |
| plan §16 | dependency list | numerics §7.4 (the delta) and `Cargo.toml` |
| plan §17.2 / §17.3 | tooling tests; the synthetic suite | **B.8** |
| plan §17.4 | the acceptance suite | **B.9** |
| plan §19 | GPU phases GK/GG | **B.10** |
| P-1.1, P-2.1, P-3.x, P-4.x | the record's phase ids | A-§ rows by date; X-n in §6.4 |

## B.1 ID semantics beyond SPEC geometry §9 (record §5.3–§5.5)

- **`N_ID`** is a set of component `X` values only (plus `0` where background-adjacent); `Y` is an
  attribute of `X` in the component table, never stored per node. `N_ID(node)` = union of the
  resolved labels of incident tets ∪ `X` of every tagged face incident to it. Examples: interface
  node between `{3}` and `{0}` → `{0, 3}`; node on the intersection curve of 3 and 5 → contains
  both; welded-sheet node (sheet `X = 7` in background) → `{0, 7}`. VTU: `n_id_key` into the
  node-ID-set table; INP: one NSET per `X`.
- **Partition IDs (R-D2):** flood fill over the final tets, adjacency blocked **only** by
  sheet-tagged faces (material interfaces do not block); deterministic numbering by ascending
  smallest node key; sheets clipped to the box, separation discovered not assumed; `[V8]` reports
  per-partition volume and label composition, a pinhole heuristic (boundary edges of a sheet
  strictly inside the domain that are neither rim, intersection-curve nor box-clip edges), and an
  optional `expected_partitions` gate.
- **Worked examples:** sphere A (`Y=1`) overlapping sphere B (`Y=2`): A∩B → `{A}`, B\A → `{B}`,
  A-surface nodes inside B `{A, B}`, B's surface inside A an inactive patch with no tags. Spheres
  A, A′ both `Y=1`: A∩A′ → `{A, A′}`, lens-edge curve nodes `{0, A, A′}`. Open sheet S (`X=9`)
  splitting the box: all `{0}`, sheet nodes `{0, 9}`, partitions 1 and 2. Void V (`Y=0`) inside
  particle P (`Y=1`): V region → `{V}`, mapped to a void material at export.
- **Reference-compat export mapping** (cross-validation only): particle components → densities
  1–999 in `X` order, `{0}` → 1000, designated void components → 1000 + index.

## B.2 Determinism contract (record §12; rules in SPEC numerics §8)

- **`strict` (default):** GPU kernels produce only conservative candidate supersets, rankings and
  diagnostics; every committed number (coordinates, accepted snaps, smoothing positions,
  classifications) is recomputed on CPU in f64 with exact predicates. Two runs — any backend mix,
  any device — produce bit-identical primary VTUs. CI gate: forced-CPU vs auto byte-compare after
  canonical ordering (`[V11]` strict).
- **`fast` (opt-in):** GPU numeric results may commit; guaranteed identical topology, labels, tag
  tables and conformance; coordinates within `1e-6 · diagonal`; quality within 1 % (`[V11]`
  topology mode). The mode is stamped in the VTU (`DeterminismMode`).
- **CPU parallelism (R-P1/R-P2):** per-item work writes into indexed buffers combined in index
  order; float reductions never use adaptive splitting; registries are `BTreeMap` or fixed-seed
  and sorted; no result depends on thread count or completion order.

## B.3 GPU/CPU responsibility split (record §13) — for M-7

**GPU proposes over regular bulk data; CPU decides everything exact, topological or sequential.**
Every GPU path is feature-gated with a rayon CPU reference; every stage's CPU path is accepted
before its kernel lands; CUDA is prohibited (project rule).

| stage | GPU (f32, filtered) | CPU (f64 + exact) |
|---|---|---|
| S0 | — | welding, repair, components |
| S2 | broad-phase candidate pairs (`mg_broadphase`: hybrid grid sized by median triangle extent, oversized-triangle side list on CPU QBVH, auto-switch to CPU QBVH when p95/p50 > 10 or oversized > 5 %) | corefinement, registry, overlay, curves |
| S2b | GWN bulk evaluation (margin band → CPU) | rebuild, radial ordering |
| S3 | bulk ray casts (`mg_rays`) | closest-pair sweep, calibration, battery, segmentation |
| S4 | curvature metrics, min-propagation (`mg_field`) | level commit, balance, coupling loop |
| S5 | Morton coding, split/balance flags (`mg_flags`) | template emission, dedup |
| S6 | parity votes + margin certificates (`mg_rays`) | margin-band exact resolution, records |
| S7 | edge-crossing candidates (`mg_flags`) | exact classification, constructions, commits |
| S8 | — | everything (rayon over independent cells) |
| S9 | AR/dihedral metrics, smoothing candidates + rankings (`mg_quality`) | all decisions and (strict) all committed numbers |
| S10 | — | labels, flood fill |
| S11 / verify | bulk distance metrics for `[V5]` | report assembly |
| render | opaque preview rasterisation | reference renderer, exact transparency |

Chunked dispatch against `max_storage_buffer_binding_size` everywhere; grow-only staging; every
GPU failure falls back to CPU with the effective backend and reason reported.

## B.4 Scale and memory envelope (record §14) — for M-6.5

Iteration-1 targets, checked by a pre-flight estimator that WARNs and by the M-6.5 benchmarks:
input ≤ 2 M triangles (hard warn 5 M); arranged ≤ 2× input; candidate pairs bounded by the hybrid
broad phase (pair count > 50× triangle count aborts naming the components); octree leaves ≤ 8 M;
**tets ≤ 20 M nominal** (warn at 40 M). Budgets ≈ 44 B/tet + 72 B/node → ≈ 1.5 GB mesh state at
20 M tets; GPU buffers chunked to the adapter's binding limit (128 MB-class assumed);
classification chunked over vertices × components; a volume snapshot ≈ 60 B/tet, hence
`snapshots: key` by default and a WARN when `all` meets a > 5 M-tet estimate; the renderer
extracts boundary faces only. Out-of-core is out of scope.

## B.5 S9 — Quality (record §10.12) — for M-5.4

Round-based IQD (the reference's deterministic variant; the legacy priority-queue path not ported)
with the ported defect taxonomy and thresholds; a record-level collapse guard; constrained
smoothing with `nc_surface / nc_polyline / nc_corner` constraints and a ring-misfit guard;
Phase-5.5 snap (2 %, AR < 10, ≥ 10°); hole-fill with majority records. Under `strict`, GPU
smoothing kernels emit candidate positions and rankings only; the commit path recomputes the
accepted position and every acceptance check in f64 on CPU. **This plan adds (M-5.4):** every
operation is bounded by P3 — a node on a tagged face or a curve never moves, so the constraint
arrays must first be made truthful for cut nodes (`Free` today), and the census (M-5.1) runs
before any operation is chosen because the defect is two-dimensional first.

## B.6 S10 — Regions, sheets taxonomy, partitions (record §10.13) — for M-6.1

Labels, `N_ID` and partitions as B.1. **Welded-sheet semantics, explicit:** a welded sheet shares
nodes — C0-continuous, no displacement discontinuity by itself — and provides (a) geometric
partition metadata, (b) material-interface tagging, (c) grain-boundary tagging, (d) an attachment
surface for cohesive/contact/crack treatments. `FaceTagSideElems` (elem⁺, elem⁻ per tagged face) is
the reserved contract that lets those treatments, including split-node generation, proceed without
re-deriving adjacency. **This plan (D-4, M-6.6):** split-node export is a user config option at
S11 (`output.interface: welded | split | cohesive`), not a meshing mode; the VTU contract stays
welded. Under `cohesive` (M-6.7, §12.1) a collapsed sheet's copies are placed back on their walls
from `SheetPairOffset` and joined by one cohesive wedge per sheet face.

## B.7 S11 — Export (record §10.14) — for M-6.2

**Domain trim (R-D1) — S11 owns it.** The background lattice covers a cube around the domain and
overhangs it by up to one coarse cell per axis by construction; no earlier stage may trim it,
because dropping cells that miss the box falsifies SPEC geometry §3.1's L1 and breaks lattice
conformity (the record measured that failure). S11 performs the final box clip of the volume,
capping the six planes the way G2-5 caps the surface; every stage before it legitimately overhangs,
which is why `[V3]`'s boundary-leak rule measures against the octree hull. (Every current domain
is the unit cube, so the trim has never fired — A-12, M-4.4.)

Primary VTU per the contract. Abaqus INP: `C3D4`; ELSET per region key and per partition; NSET per
component `X`; `*SURFACE` per sheet and per interface pair from the face-tag table's side elements.
**Material mapping:** every region key in the mesh must resolve through `materials.by_component`
(singleton keys) or `materials.by_region_key` (multi-ID keys); an unmapped multi-ID key is a hard
error listing the keys (`unmapped: elset-only` exports the ELSET with no section and WARNs). No
silent single-component assignment, ever. Reference-compat mapping (B.1) for cross-validation.
`[V10]` cross-checks INP ↔ VTU: element and node counts, per-set sums, unmapped-region audit.

## B.8 The synthetic test suite (record §17.3) — for M-6.4

- **Predicate/unit:** orient3d degeneracies; parity re-shoot; registry identity (same provenance ⇒
  same vertex, adjacent triangles agree); write-policy cells; `resolve()` truth table; template
  signed volumes (Freudenthal, fans, band k-cases, junction fixtures).
- **Tooling self-tests:** corrupted-VTU fixture suite; VTU round-trip byte-stable; renderer image
  regression (synthetic scenes × filters × transparency × clipping × named views vs committed PNGs).
- **Synthetic geometry (generators committed):** overlapping spheres same-Y / different-Y;
  sphere-in-sphere; gap sweep `2·t_layer → 0.5·t_sheet`; sharp cube; thin flake; open plane sheet
  (2 partitions); two crossing sheets (4 partitions); sheet ∩ sphere; sub-ε-pinhole sphere; open
  hemisphere; degenerate-triangle soup; dumbbell + branching throat; partial coplanar overlap; fully
  coincident surfaces, equal and opposite normals; tangential sphere contact; vertex-only contact;
  edge-only contact; surface T-junction (sheet ending on a solid wall); self-intersecting nominally
  closed STL; three surfaces meeting along one line; four regions meeting in one cell; sheet–sheet–
  solid junction; sheet terminating inside the domain; sheet coincident with a domain face;
  long-skinny input triangles (AR ~10³); highly nonuniform triangle sizes; curved thin gap
  (concentric spheres); oblique thin gap (angled plates) — the thin ones now
  also end to end, through S8b, as A-17..A-20 (M-4.9); same-priority overlap without a material
  mapping (INP export must fail with the key list); scale invariance — one geometry at m/mm/µm must
  yield identical topology and labels.
- **Reference cross-validation:** the three intersection cases and the yarn case under the compat
  mapping; per-region volume fractions, interface areas, `[V4]`/`[V5]` parity or better; void-slit
  survival; mismatch triage.
- **FEM smoke:** assembly + Laplace/elastic solve on exported meshes (welded sheets, multi-ID with
  mapping, A-7a under `cohesive` with a stiff traction–separation law); no singular Jacobians;
  field continuity across welded interfaces and, across a cohesive layer, up to its own compliance.
- **Determinism + parity:** two-run byte-compare; forced-CPU vs auto under strict (`[V11]`);
  cross-device GPU comparison under fast; GPU tests skip gracefully without hardware.
- **Performance/memory:** stage timings on the three intersection cases + a resolution sweep; the
  B.4 envelope estimator audited.

## B.9 The acceptance suite (record §17.4, extended) — for M-4.4, M-4.7, M-6.4

Each case ships as a committed generator (analytic STL, so volume and area are exact), a config,
and an expected-outcome record; the four thin/lattice cases, A-10 and the thin family A-17..A-20
also ship a cross-section render. Configs: `h_max_frac = 0.05`, `h_min_frac = 0.012`, with the recorded per-case exceptions
(A-4 0.0125, A-6 0.04/0.004, A-8 `h_min` 0.006) until M-4.6 replaces them with a derived
background and level (R-E4); a fixture that exists to exercise a regime the default options do
not choose (R-E6) sets the option, or keeps explicit `background`/`max_level`, and says why.

| # | case | geometry | validates |
|---|---|---|---|
| A-1 | sphere | one closed sphere in a box | baseline: curvature sizing, one component, `{X, Y}`, volume vs `4/3·π·r³` |
| A-2 | cube | one axis-aligned cube | sharp features: 12 curves and 8 corners captured, flat faces not over-refined, exact volume |
| A-3 | sphere ∩ cube | interpenetrating; **run twice: equal `Y` and ranked** | intersections: the curve explicit and conforming; equal `Y` → three non-background keys, the lens `{sphere, cube}` (R-A3); ranked → two keys, the lens to the winner (R-A4); volumes sum to the union. **The ranked run has never been made** |
| A-4 / A-4b | cube inside sphere, equal `Y` / ranked | cube strictly interior | containment: A-4 → `{sphere}` shell and `{sphere, cube}` interior with the right volumes, no spurious curve; A-4b → `{cube}` and `{sphere}`, no shared key. **A-4b never built** |
| A-5 | sphere inside cube | both `Y = 0` | container/contained symmetry in `resolve()` and the partition fill. **Never built** |
| A-6a / A-6b | large cube + thin limb | limb thickness `≤ t_sheet` / in `(t_sheet, t_layer]` | thin features: the limb survives as a collapsed sheet / a one-layer band, never lost; grading without a 2:1 violation; `[V7]` clean. **Neither regime has ever fired (M-4.3)** |
| A-7a / A-7b | two bodies, tiny gap | gap `≤ t_sheet` / in `(t_sheet, t_layer]` | near-contact: band → sheet → contact; one element layer while resolvable; regime locking; the bodies never weld *silently*, and under `output.interface: cohesive` never weld at all — the gap delivered as a cohesive layer on both walls (D-8, §12.1, M-6.7). **Neither regime has ever fired (M-4.3)** |
| A-8 | strut lattice | periodic struts in a box | many junctions at once; escalation rate; meshed connectivity equals the input's |
| A-9 | lattice ∩ / ⊃ sphere or cube | a lattice interpenetrating and containing a body | all five situations at realistic scale. **Never built** |
| **A-10** | forged contact *(new)* | two flattened solids whose opposing faces close from a wide gap to contact at the centre (ellipsoids, or two spheres through the repository's `forge` pipeline) | G4: volumetric → band → sheet → marked contact in one mesh; `[V7]` non-vacuous; the interface exported per `output.interface` (M-4.7, M-6.6) |
| **A-11** | open sheet crossing the box *(new)* | an open planar or curved sheet through the domain | R-C1, R-C3, R-D2: no volume inside the sheet, rim conforming, `[V8]` partitions = 2 |
| **A-12** | non-cubic domain *(new)* | any existing case in a `[0,1]×[0,1]×[0,0.6]` box | R-D1: no cell outside the box after S11 |
| **A-13** | defective input *(new)* | a sphere with a duplicated triangle, a flipped one and a sub-`ε` gap | topic M: each `repair` level's action log; `[V1]`/`[V3]` PASS |
| **A-14a / A-14b** | mixed single file *(new, MG-04)* | one STL holding two disjoint closed cubes; one STL holding a closed cube and a disjoint open quad | R-A2: two X from one file; solid + sheet in one file keeps the solid's volume; the source map file → shells → X |
| **A-15** | borrowed closure *(new, MG-05)* | a five-face cube declared `solid` and an independent sheet exactly over the missing face | S2b: the solid stays defective (GWN path, WARN); the sheet's identity is its own; shared geometry changes neither record |
| **A-16** | fully merged component *(new, D-42)* | two identical closed cubes at equal priority (`a2_cube.stl` listed twice) | S2b classifies both as solids; S6 gives the whole cube region key `{1, 2}` (R-A3). Today: one solid plus one sheet, and the second cube's material is lost |
| **A-17m / A-17g** | tilted thin plate / gap *(new, R-B3)* | a flat plate of uniform thickness, material in a matrix box / as the gap between two blocks, at five orientations: lattice-aligned, 17°, 45°, a generic (17°, 29°, 41°) rotation, and walls on a lattice facet plane | the thin path is orientation-invariant: each regime's area share, volume error and elements per area agree with the lattice-aligned control within one cell of rim; the facet-plane walls mesh with `[V2]` clean (M-4.9) |
| **A-18m / A-18g** | irregular thin plate / gap *(new, R-B3)* | a curved mid-surface (saddle, cylindrical patch) with a non-convex outline — a re-entrant corner and a hole — at uniform thickness | rims along an irregular outline; a curved mid-surface validated or its refusal named; `[V3]` clean across every rim (M-4.9) |
| **A-19m / A-19g** | non-uniform thin plate / gap *(new, R-B3)* | thickness tapered from above `t_layer` to below `t_sheet` with a ripple on it; the gap variant a flat wedge closing to contact along one edge, and a rippled gap | regime boundaries where the analytic thickness crosses `t_sheet` and `t_layer`, within one cell, every boundary crossed more than once (M-4.9; A-10 is the curved, forged version) |
| **A-20m / A-20g** | the real flake *(new, R-B3)* | tilted, curved, irregular and tapered at once, as material and as a gap between two bodies | all of A-17..A-19 at once, under each value of the thin options (R-E6, M-4.6, M-4.9) |

Acceptance for every case: `mesh-verify` `fail = 0` on the **exported** mesh with `[V3]` clean
(R7); volume and interface area within contracts §5; byte-identical at 1 and 8 threads; the mesh
assembles in a solver (B.8's FEM smoke); green twice consecutively.

## B.10 GPU phases (record §19 Phases GK and GG) — M-7

- **GK-1. `mg_broadphase` + `mg_rays`** (S2/S3/S6) with margin certificates. *Acceptance:*
  decision-identical vs CPU under strict; indicative ≥ 5× on the 1 M-triangle case.
- **GK-2. `mg_field` + `mg_flags` + `mg_quality`** (S4/S5/S9), proposals-only contract audited.
- **GK-3. Determinism harness:** `[V11]` strict CI gate; fast-mode contract tests; cross-device
  comparison runs.
- **GG-1. GPU-enabled performance validation** — last, on real hardware (this host has none; wgpu
  falls back to llvmpipe, and a batch that takes 0.14 s on CPU takes 12.5 s there, so no timing
  measured here is representative). Assert the adapter is not a software rasteriser; record the
  environment (adapter, backend, driver, `Limits`, cores, RAM); re-baseline the CPU path on the
  same host and re-run the 1-vs-N-thread byte check; apply and measure the five GPU-review findings
  in order — per-view allocations in `render_views`, the submit-then-stall pattern, per-call
  staging buffers, workgroup sizes (64 vs 256), chunked dispatch against the binding limit — each
  with a before/after number or an explicit "measured, not a problem on this hardware"; exercise
  every GPU path with real work against the CPU reference for time and output; verify strict-mode
  bit-identity with the GPU active and across two adapters if available; audit the B.4 envelope.
  *Acceptance:* a benchmark table with the host identified; every finding closed with a number; if
  a GPU path is slower than its CPU reference, say so and record the crossover size.

# Appendix C — the review's reproduction script (2026-09-11), verbatim

Saved as `reproduce.py` and run as `python3 reproduce.py <repo root>` with the release binary
built. It writes every input, config, snapshot and JSON under a fresh `run-*` directory beneath
`BASE` (the review used `/tmp/rustmspt-mesh-review`; the 2026-09-23 re-run pointed `BASE` at the
session scratchpad and changed nothing else), depends only on the Python standard library and the
binary, and clears every `RUSTMSPT_*` variable from the subprocess environment so the default path
runs. It records the **pre-fix** counterexamples; M-1.0, M-1.5, M-4.0 and M-4.8 turn each of its
invariants into a committed fixture that asserts the corrected behaviour, and the script is then
retired.

```python
import copy
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from fractions import Fraction

ROOT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else '.').resolve()
BASE = pathlib.Path('/tmp/rustmspt-mesh-review')
BASE.mkdir(parents=True, exist_ok=True)
OUT = pathlib.Path(tempfile.mkdtemp(prefix='run-', dir=BASE))
BIN = ROOT / 'target/release/rustmspt'
FIX = ROOT / 'data/fixtures/meshgen'
RUN_ENV = {key: value for key, value in os.environ.items() if not key.startswith('RUSTMSPT_')}
print('artifacts:', OUT, flush=True)


def array(tree, name):
    return tree.find(f'.//DataArray[@Name="{name}"]')


def cube(lo, hi):
    p = [(hi[0] if i & 1 else lo[0], hi[1] if i & 2 else lo[1], hi[2] if i & 4 else lo[2]) for i in range(8)]
    faces = [(0, 2, 3), (0, 3, 1), (4, 5, 7), (4, 7, 6),
             (0, 1, 5), (0, 5, 4), (2, 6, 7), (2, 7, 3),
             (0, 4, 6), (0, 6, 2), (1, 3, 7), (1, 7, 5)]
    return [[p[i] for i in f] for f in faces]


def stl(name, tris):
    path = OUT / (name + '.stl')
    with path.open('w') as f:
        f.write('solid review\n')
        for tri in tris:
            f.write('facet normal 0 0 0\nouter loop\n')
            for p in tri:
                f.write('vertex ' + ' '.join(map(str, p)) + '\n')
            f.write('endloop\nendfacet\n')
        f.write('endsolid review\n')
    return path


def verify(name, tree, gates=None, surfaces=None):
    path = OUT / (name + '.vtu')
    tree.write(path, encoding='unicode')
    cfg = {'mesh_verify': {'input': str(path), 'json': str(OUT / (name + '.json')),
                           'verify': gates or {}, 'surfaces': list(map(str, surfaces or []))}}
    cfg_path = OUT / (name + '.yaml')
    cfg_path.write_text(json.dumps(cfg))
    proc = subprocess.run([str(BIN), 'mesh-verify', '--config', str(cfg_path)], env=RUN_ENV, capture_output=True, text=True, timeout=60)
    (OUT / (name + '.log')).write_text(proc.stdout + proc.stderr)
    report = json.loads((OUT / (name + '.json')).read_text())
    sections = {s['id']: s for s in report['sections']}
    result = {'case': name, 'exit': proc.returncode, 'summary': report['summary'],
              'status': {k: v['status'] for k, v in sections.items()},
              'V3': sections['V3']['metrics'], 'V13': sections['V13']['metrics']}
    print(json.dumps(result))
    return result


good = ET.parse(FIX / 'good_cube.vtu')
reference = stl('unit_cube', cube((0, 0, 0), (1, 1, 1)))
results = []
results.append(verify('corner_only', copy.deepcopy(good), surfaces=[reference]))

large = copy.deepcopy(good)
coords = list(map(float, array(large, 'Points').text.split()))
for i in range(0, len(coords), 3):
    coords[i] *= 2
array(large, 'Points').text = ' '.join(map(str, coords))
results.append(verify('final_outside_domain', large))

results.append(verify('hidden_failure', ET.parse(FIX / 'bad_inverted_tet.vtu'), {'max_items_per_section': 0}))

missing = copy.deepcopy(good)
pd = missing.find('.//PointData')
pd.remove(array(missing, 'constraint_kind'))
pd.remove(array(missing, 'constraint_ref'))
results.append(verify('missing_required', missing))

bad_sides = copy.deepcopy(good)
array(bad_sides, 'FaceTagSideElems').text = '999999 999998 999999 999998'
results.append(verify('invalid_side_elements', bad_sides))

sheet_owns = copy.deepcopy(good)
array(sheet_owns, 'ComponentKind').text = '1'
results.append(verify('sheet_claims_volume', sheet_owns))

# The GPU G1 filter bounds arithmetic on rounded coordinates, not f64 -> f32 conversion.
f32 = lambda x: struct.unpack('f', struct.pack('f', x))[0]
sub = lambda a, b: f32(a - b)
mul = lambda a, b: f32(a * b)
add = lambda a, b: f32(a + b)
a = (0.1, 0.1, 0.5 - 2e-8)
b = (0.5, 0.1, 0.5 + 2e-8)
c = (0.1, 0.5, 0.5 + 2e-8)
p = (0.15, 0.15, 0.5 - 1.1e-8)
af, bf, cf, pf = [tuple(map(f32, v)) for v in (a, b, c, p)]
ad, bd, cd = [[sub(v[i], pf[i]) for i in range(3)] for v in (af, bf, cf)]
terms = [(mul(bd[0], cd[1]), mul(cd[0], bd[1]), ad[2]),
         (mul(cd[0], ad[1]), mul(ad[0], cd[1]), bd[2]),
         (mul(ad[0], bd[1]), mul(bd[0], ad[1]), cd[2])]
det = add(add(mul(terms[0][2], sub(terms[0][0], terms[0][1])),
              mul(terms[1][2], sub(terms[1][0], terms[1][1]))),
          mul(terms[2][2], sub(terms[2][0], terms[2][1])))
perm = add(add(mul(abs(terms[0][2]), add(abs(terms[0][0]), abs(terms[0][1]))),
               mul(abs(terms[1][2]), add(abs(terms[1][0]), abs(terms[1][1])))),
           mul(abs(terms[2][2]), add(abs(terms[2][0]), abs(terms[2][1]))))
aq, bq, cq = [[Fraction(v[i]) - Fraction(p[i]) for i in range(3)] for v in (a, b, c)]
exact = (aq[2] * (bq[0] * cq[1] - cq[0] * bq[1]) +
         bq[2] * (cq[0] * aq[1] - aq[0] * cq[1]) +
         cq[2] * (aq[0] * bq[1] - bq[0] * aq[1]))
gpu = {'case': 'gpu_g1', 'f32_det': det, 'f64_exact_det': float(exact),
       'filter_bound': 4.172e-7 * perm, 'filter_accepts': abs(det) > 4.172e-7 * perm,
       'opposite_sign': det * float(exact) < 0}
print(json.dumps(gpu))
results.append(gpu)
(OUT / 'results.json').write_text(json.dumps(results, indent=2))

name = 'contact_orientation'
left = stl('contact_left', cube((0.1, 0.1, 0.1), (0.4, 0.4, 0.4)))
right = stl('contact_right', cube((0.4, 0.1, 0.1), (0.7, 0.4, 0.4)))
cfg = {'meshgen': {'inputs': [{'stl': str(left)}, {'stl': str(right)}],
                   'domain': {'min': [0, 0, 0], 'max': [1, 1, 1]},
                   'sizing': {'h_max_frac': 0.2, 'h_min_frac': 0.05}, 'snapshots': 'all',
                   'output': {'vtu': str(OUT / (name + '.vtu'))}}}
cfg_path = OUT / (name + '.yaml')
cfg_path.write_text(json.dumps(cfg))
proc = subprocess.run([str(BIN), 'mesh', '--config', str(cfg_path)], env=RUN_ENV, capture_output=True, text=True, timeout=60)
(OUT / (name + '.log')).write_text(proc.stdout + proc.stderr)
doc = ET.parse(OUT / (name + '.debug') / (name + '_s08_cut_contract.vtu'))
offsets = list(map(int, array(doc, 'FaceTagOffsets').text.split()))
result = {'case': name, 'exit': proc.returncode, 'tag_sets': len(offsets),
          'multi_tag_sets': sum(b - a > 1 for a, b in zip([0] + offsets, offsets)),
          'tag_members': len(array(doc, 'FaceTagComponents').text.split()),
          'orientations': len(array(doc, 'FaceTagOrientation').text.split())}
print(json.dumps(result))
results.append(result)
(OUT / 'results.json').write_text(json.dumps(results, indent=2))

whole = cube((0.1, 0.1, 0.1), (0.3, 0.3, 0.3))
open_solid = stl('open_solid', whole[:2] + whole[4:])
cap_sheet = stl('cap_sheet', whole[2:4])
name = 'borrowed_closure'
cfg = {'meshgen': {'inputs': [{'stl': str(open_solid), 'kind': 'solid'},
                              {'stl': str(cap_sheet), 'kind': 'sheet'}],
                   'domain': {'min': [0, 0, 0], 'max': [1, 1, 1]},
                   'sizing': {'h_max_frac': 0.2, 'h_min_frac': 0.05}, 'snapshots': 'all',
                   'output': {'vtu': str(OUT / (name + '.vtu'))}}}
cfg_path = OUT / (name + '.yaml')
cfg_path.write_text(json.dumps(cfg))
proc = subprocess.run([str(BIN), 'mesh', '--config', str(cfg_path)], env=RUN_ENV, capture_output=True, text=True, timeout=60)
(OUT / (name + '.log')).write_text(proc.stdout + proc.stderr)
for stage, suffix in [('S0', 's00_conditioned'), ('S2', 's02_arranged')]:
    doc = ET.parse(OUT / (name + '.debug') / (name + '_' + suffix + '.vtu'))
    result = {'case': name, 'stage': stage, 'exit': proc.returncode,
              'components': array(doc, 'ComponentX').text,
              'kind': array(doc, 'ComponentKind').text,
              'closed': array(doc, 'ComponentClosed').text}
    print(json.dumps(result))
    results.append(result)
(OUT / 'results.json').write_text(json.dumps(results, indent=2))

# One input file containing two solids / one solid plus a disconnected sheet.
first = cube((0.1, 0.1, 0.1), (0.3, 0.3, 0.3))
second = cube((0.6, 0.6, 0.6), (0.8, 0.8, 0.8))
sheet = [[(0.6, 0.6, 0.6), (0.8, 0.6, 0.6), (0.8, 0.8, 0.6)],
         [(0.6, 0.6, 0.6), (0.8, 0.8, 0.6), (0.6, 0.8, 0.6)]]
for name, tris in [('two_solids_one_file', first + second), ('solid_and_sheet_one_file', first + sheet)]:
    source = stl(name, tris)
    cfg = {'meshgen': {'inputs': [{'stl': str(source)}], 'domain': {'min': [0, 0, 0], 'max': [1, 1, 1]},
                       'sizing': {'h_max_frac': 0.2, 'h_min_frac': 0.05}, 'snapshots': 'all',
                       'output': {'vtu': str(OUT / (name + '.vtu'))}}}
    cfg_path = OUT / (name + '.yaml')
    cfg_path.write_text(json.dumps(cfg))
    proc = subprocess.run([str(BIN), 'mesh', '--config', str(cfg_path)], env=RUN_ENV, capture_output=True, text=True, timeout=60)
    (OUT / (name + '.log')).write_text(proc.stdout + proc.stderr)
    stage0 = ET.parse(OUT / (name + '.debug') / (name + '_s00_conditioned.vtu'))
    result = {'case': name, 'exit': proc.returncode,
              'components': array(stage0, 'ComponentX').text,
              'kind': array(stage0, 'ComponentKind').text,
              'closed': array(stage0, 'ComponentClosed').text}
    print(json.dumps(result))
    results.append(result)
(OUT / 'results.json').write_text(json.dumps(results, indent=2))
```


# Appendix D — the rev 3.3 scripts (2026-09-25), verbatim

Two stand-alone scripts, standard library only, behind every number rev 3.3 adds. They are kept
here rather than in the repository because rev 3.3 is a plan-only revision; the subtasks that own
them (M-1.7 and M-1.8) commit them beside their tests, and geometry rev 1.7's §14 record cites the
committed copies.

## D.1 `lattice_pattern.py` — the three split schemes through §3's transition machinery

Builds geometry §2.2's table (Rule D), its parity-reflected instantiation (Rule X) and the 5-tet
parity checkerboard (Rule T5) as leaves of an integer octree on a doubled grid, applies §3.1's split
state and §3.3's face rule (P, E, Q) with each scheme's own diagonal rule, fans every fan cell to its
centroid, and reports per-template quality, conformity on seven strongly balanced configurations
plus an unbalanced negative control, a uniform block's edge directions, vertex valences and `{110}`
/ `{111}` facet planes, and the exact volumes one plain leaf emits. The Rule D rows reproduce §3.7's
frozen table to every printed digit, which is the script's own check. **D-9 chose the checkerboard
(T5)**; the Rule X rows are the declined alternative, kept for the record. On the 6³ block the
all-even and all-odd interior vertices are 27 + 8 = 35 of 125; on an unbounded lattice they are a
quarter, so Rule X's mean valence is `0.25·26 + 0.75·10 = 14`, Rule D's everywhere, and the
checkerboard's `0.5·18 + 0.5·6 = 12` (six edges per cube instead of seven).

```python
#!/usr/bin/env python3
"""Lattice split-pattern enumeration for the plan (stdlib only).

Three plain-cell schemes, each with the face rule its plain cells induce, run through
SPEC geometry section 3's transition machinery (split state, face rule P/E/Q, centroid fan):

  D  - uniform Freudenthal (the frozen section 2.2 table on every cube; Rule D:
       min corner to max corner)
  X  - parity-reflected Freudenthal (the section 2.2 table relabelled v_i -> v_(i xor m)
       where m's bits are the parities of the cube's own-level index; Rule X: the
       diagonal joins the two face corners whose in-plane own-level indices have
       equal parity)
  T5 - the 5-tet parity checkerboard (central tet on the four corners whose
       own-level index sum is even; Rule T5: the diagonal joins the two face corners
       whose own-level index sum, normal coordinate included, is even)

Coordinates are integers on a doubled grid, so every predicate is exact.
Reported: per-template quality (exact-arithmetic dihedrals via floats of exact
vectors; AR = R/(3 r_in)), conformity of seven octree configurations (every
interior triangle shared by exactly two tets, no node inside another triangle or
edge, volume exact, every tet positive), and uniform-block isotropy statistics.
"""
import itertools
import math
from collections import Counter, defaultdict
from fractions import Fraction

# ----------------------------------------------------------------- geometry

def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def orient(t):
    return dot(sub(t[1], t[0]), cross(sub(t[2], t[0]), sub(t[3], t[0])))


def positive(t):
    t = list(t)
    o = orient(t)
    assert o != 0, f"degenerate tet {t}"
    if o < 0:
        t[1], t[2] = t[2], t[1]
    return tuple(t)


def dihedrals(t):
    out = []
    for i, j in itertools.combinations(range(4), 2):
        k, l = [m for m in range(4) if m not in (i, j)]
        e = sub(t[j], t[i])
        ee = dot(e, e)
        u = sub(t[k], t[i])
        v = sub(t[l], t[i])
        up = [Fraction(u[a]) - Fraction(dot(u, e), ee) * e[a] for a in range(3)]
        vp = [Fraction(v[a]) - Fraction(dot(v, e), ee) * e[a] for a in range(3)]
        c = float(sum(up[a] * vp[a] for a in range(3))) / math.sqrt(
            float(sum(x * x for x in up)) * float(sum(x * x for x in vp)))
        out.append(math.degrees(math.acos(max(-1.0, min(1.0, c)))))
    return out


def aspect_ratio(t):
    p0 = t[0]
    rows = []
    rhs = []
    for p in t[1:]:
        d = sub(p, p0)
        rows.append([Fraction(2 * x) for x in d])
        rhs.append(Fraction(dot(p, p) - dot(p0, p0)))
    # Cramer's rule, exact
    def det3(m):
        return (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))
    d = det3(rows)
    c = []
    for col in range(3):
        m = [r[:] for r in rows]
        for r in range(3):
            m[r][col] = rhs[r]
        c.append(det3(m) / d)
    R = math.sqrt(float(sum((c[a] - p0[a]) ** 2 for a in range(3))))
    vol6 = abs(orient(t))
    area2 = 0.0
    for f in itertools.combinations(t, 3):
        n = cross(sub(f[1], f[0]), sub(f[2], f[0]))
        area2 += math.sqrt(dot(n, n))
    # r_in = 3V / A = (vol6/2) / (area2/2) = vol6 / area2
    r_in = vol6 / area2
    return R / (3.0 * r_in)


# ----------------------------------------------------------------- the lattice

# SPEC geometry 2.1: corner m has bits (bx, by, bz); 2.2's frozen table.
KUHN = [(0, 1, 3, 7), (0, 1, 7, 5), (0, 2, 7, 3), (0, 2, 6, 7), (0, 4, 5, 7), (0, 4, 7, 6)]
# 5-tet split on the even corners {0,3,5,6}; relabelled by xor 1 for the odd set.
FIVE = [(0, 3, 5, 6), (1, 0, 3, 5), (2, 0, 6, 3), (4, 0, 5, 6), (7, 3, 6, 5)]


def corner(origin, s, m):
    return (origin[0] + (m & 1) * s, origin[1] + ((m >> 1) & 1) * s, origin[2] + ((m >> 2) & 1) * s)


def plain_cell(scheme, origin, s):
    """Tets of a leaf with no split face or edge. origin, s in the doubled grid."""
    idx = [origin[a] // s for a in range(3)]  # own-level index
    if scheme == "D":
        rows, relabel = KUHN, 0
    elif scheme == "X":
        rows, relabel = KUHN, (idx[0] & 1) | ((idx[1] & 1) << 1) | ((idx[2] & 1) << 2)
    elif scheme == "T5":
        rows, relabel = FIVE, (sum(idx) & 1)  # odd cube: central tet on the other four
    else:
        raise ValueError(scheme)
    return [positive(tuple(corner(origin, s, m ^ relabel) for m in row)) for row in rows]


def face_diagonal(scheme, quad, s, axis):
    """quad: four corners of an unsplit axis-aligned face of side s (cyclic order).
    Returns the two diagonal endpoints the scheme's face rule picks."""
    if scheme == "D":
        lo = min(quad)
        hi = max(quad)
        return lo, hi
    if scheme == "X":
        a, b = [x for x in range(3) if x != axis]
        for p in quad:
            q = [r for r in quad if r[a] != p[a] and r[b] != p[b]][0]
            if (p[a] // s) % 2 == (p[b] // s) % 2:
                return p, q
        raise AssertionError
    if scheme == "T5":
        for p in quad:
            q = [r for r in quad if all(r[x] != p[x] for x in range(3) if x != axis)][0]
            if sum(c // s for c in p) % 2 == 0:
                return p, q
        raise AssertionError
    raise ValueError(scheme)


def plain_face_tris(scheme, quad, s, axis):
    p, q = face_diagonal(scheme, quad, s, axis)
    others = [r for r in quad if r not in (p, q)]
    return [(p, q, others[0]), (p, q, others[1])]


def cube_faces(origin, s):
    """Six faces as (axis, side, cyclic corner list)."""
    out = []
    for axis in range(3):
        a, b = [x for x in range(3) if x != axis]
        for side in (0, 1):
            base = list(origin)
            base[axis] += side * s
            cyc = []
            for da, db in ((0, 0), (1, 0), (1, 1), (0, 1)):
                p = list(base)
                p[a] += da * s
                p[b] += db * s
                cyc.append(tuple(p))
            out.append((axis, side, cyc))
    return out


def midpoint(p, q):
    return tuple((p[i] + q[i]) // 2 for i in range(3))


def build(scheme, leaves):
    """leaves: list of (origin, s) in the doubled grid (s even). Returns list of tets."""
    nodes = set()
    for o, s in leaves:
        for m in range(8):
            nodes.add(corner(o, s, m))
    tets = []
    for o, s in leaves:
        faces = cube_faces(o, s)
        split_any = False
        for axis, side, cyc in faces:
            c = midpoint(cyc[0], cyc[2])
            if c in nodes:
                split_any = True
            for t in range(4):
                if midpoint(cyc[t], cyc[(t + 1) % 4]) in nodes:
                    split_any = True
        if not split_any:
            tets.extend(plain_cell(scheme, o, s))
            continue
        cen = tuple(o[i] + s // 2 for i in range(3))
        for axis, side, cyc in faces:
            c = midpoint(cyc[0], cyc[2])
            tris = []
            if c in nodes:  # case Q: case P at level L+1 on each quadrant
                h = s // 2
                for t in range(4):
                    p = cyc[t]
                    qa = midpoint(cyc[t], cyc[(t + 1) % 4])
                    qb = midpoint(cyc[t], cyc[(t + 3) % 4])
                    quad = [p, qa, c, qb]
                    tris.extend(plain_face_tris(scheme, quad, h, axis))
            else:
                walk = []
                k = 0
                for t in range(4):
                    walk.append(cyc[t])
                    mid = midpoint(cyc[t], cyc[(t + 1) % 4])
                    if mid in nodes:
                        walk.append(mid)
                        k += 1
                if k == 0:  # case P
                    tris.extend(plain_face_tris(scheme, cyc, s, axis))
                else:  # case E
                    for t in range(len(walk)):
                        tris.append((c, walk[t], walk[(t + 1) % len(walk)]))
            for tri in tris:
                tets.append(positive((tri[0], tri[1], tri[2], cen)))
    return tets


def check_conformity(tets, box_lo, box_hi):
    faces = Counter()
    vol6 = 0
    for t in tets:
        o = orient(t)
        assert o > 0
        vol6 += o
        for f in itertools.combinations(t, 3):
            faces[tuple(sorted(f))] += 1
    bad_interior = 0
    for f, n in faces.items():
        on_box = any(all(p[a] == f[0][a] for p in f) and f[0][a] in (box_lo[a], box_hi[a]) for a in range(3))
        if on_box:
            if n != 1:
                bad_interior += 1
        elif n != 2:
            bad_interior += 1
    verts = set(p for t in tets for p in t)
    hanging = 0
    for f in faces:
        a, b, c = f
        n = cross(sub(b, a), sub(c, a))
        lo = [min(p[i] for p in f) for i in range(3)]
        hi = [max(p[i] for p in f) for i in range(3)]
        for v in verts:
            if v in f or any(v[i] < lo[i] or v[i] > hi[i] for i in range(3)):
                continue
            if dot(n, sub(v, a)) != 0:
                continue
            # v in the closed triangle, not a vertex -> hanging
            w = [dot(n, cross(sub(f[(i + 1) % 3], f[i]), sub(v, f[i]))) for i in range(3)]
            if all(x >= 0 for x in w) or all(x <= 0 for x in w):
                hanging += 1
    box_vol6 = 6 * (box_hi[0] - box_lo[0]) * (box_hi[1] - box_lo[1]) * (box_hi[2] - box_lo[2])
    return bad_interior, hanging, vol6 == box_vol6


# ----------------------------------------------------------------- templates

def template_quality(scheme):
    rows = {}
    # plain cell, every parity of its own-level index
    plain = []
    for idx in itertools.product((0, 1), repeat=3):
        plain.extend(plain_cell(scheme, tuple(2 * i for i in idx), 2))
    rows["plain cell"] = plain
    # fan cell of side 4 at origin index parity p; fan over P, Q and E faces
    fanP, fanQ, fanE = [], [], []
    for idx in itertools.product((0, 1), repeat=3):
        o = tuple(4 * i for i in idx)
        s = 4
        cen = tuple(o[i] + 2 for i in range(3))
        for axis, side, cyc in cube_faces(o, s):
            for tri in plain_face_tris(scheme, cyc, s, axis):
                fanP.append(positive((*tri, cen)))
            c = midpoint(cyc[0], cyc[2])
            for t in range(4):
                p = cyc[t]
                qa = midpoint(cyc[t], cyc[(t + 1) % 4])
                qb = midpoint(cyc[t], cyc[(t + 3) % 4])
                for tri in plain_face_tris(scheme, [p, qa, c, qb], 2, axis):
                    fanQ.append(positive((*tri, cen)))
            walk = []
            for t in range(4):
                walk.append(cyc[t])
                if t == 0:
                    walk.append(midpoint(cyc[0], cyc[1]))
            for t in range(len(walk)):
                fanE.append(positive((c, walk[t], walk[(t + 1) % len(walk)], cen)))
    rows["fan over a P face"] = fanP
    rows["fan over a Q quadrant triangle"] = fanQ
    rows["fan over an E face (k=1)"] = fanE
    out = {}
    for name, ts in rows.items():
        dmin = min(min(dihedrals(t)) for t in ts)
        dmax = max(max(dihedrals(t)) for t in ts)
        ar = max(aspect_ratio(t) for t in ts)
        out[name] = (dmin, dmax, ar, len(ts))
    return out


# ----------------------------------------------------------------- configurations

def uniform(n, s=2, origin=(0, 0, 0)):
    return [((origin[0] + i * s, origin[1] + j * s, origin[2] + k * s), s)
            for i in range(n) for j in range(n) for k in range(n)]


def refine(leaves, target):
    out = []
    for o, s in leaves:
        if (o, s) == target:
            h = s // 2
            for m in range(8):
                out.append((corner(o, h, m), h))
        else:
            out.append((o, s))
    return out


def configurations():
    c = {}
    base = uniform(2, 4)  # 2x2x2 coarse block, side 4 (doubled grid), box [0,8]^3
    c["uniform 4^3"] = (uniform(4, 2), (0, 0, 0), (8, 8, 8))
    c["one refined corner cell"] = (refine(base, ((0, 0, 0), 4)), (0, 0, 0), (8, 8, 8))
    c["two refined, face-adjacent"] = (refine(refine(base, ((0, 0, 0), 4)), ((4, 0, 0), 4)), (0, 0, 0), (8, 8, 8))
    c["two refined, edge-only"] = (refine(refine(base, ((0, 0, 0), 4)), ((4, 4, 0), 4)), (0, 0, 0), (8, 8, 8))
    c["two refined, vertex-only"] = (refine(refine(base, ((0, 0, 0), 4)), ((4, 4, 4), 4)), (0, 0, 0), (8, 8, 8))
    big = uniform(3, 4)  # 3x3x3, box [0,12]^3, centre refined
    c["centre of 3^3 refined"] = (refine(big, ((4, 4, 4), 4)), (0, 0, 0), (12, 12, 12))
    # three levels under strong 2:1 balance: a 4^3 block of side-8 cells, its central
    # 2x2x2 refined to side 4, then one side-4 cell that touches no side-8 cell refined
    # to side 2 (a corner child of one coarse cell would touch level 0 - unbalanced)
    lv = uniform(4, 8)
    for o in itertools.product((8, 16), repeat=3):
        lv = refine(lv, (tuple(o), 8))
    c["three levels, balanced"] = (refine(lv, ((12, 12, 12), 4)), (0, 0, 0), (32, 32, 32))
    # the unbalanced one, kept as the negative control: every scheme must fail it
    neg = refine(big, ((4, 4, 4), 4))
    neg = [((o[0] * 2, o[1] * 2, o[2] * 2), s * 2) for o, s in neg]
    c["NEGATIVE: level 2 touching level 0"] = (refine(neg, ((8, 8, 8), 4)), (0, 0, 0), (24, 24, 24))
    return c


def isotropy(scheme, n=6):
    tets = build(scheme, uniform(n, 2))
    edges = set()
    for t in tets:
        for a, b in itertools.combinations(t, 2):
            edges.add(tuple(sorted((a, b))))
    kinds = Counter()
    for a, b in edges:
        d = sub(b, a)
        nz = sum(1 for x in d if x != 0)
        if nz == 1:
            kinds["axis"] += 1
        else:
            g = [x // 2 for x in d]
            # canonical direction up to sign
            if g < [-x for x in g]:
                g = [-x for x in g]
            kinds[("face" if nz == 2 else "body") + str(tuple(g))] += 1
    val = defaultdict(int)
    for a, b in edges:
        val[a] += 1
        val[b] += 1
    interior = [v for v in val if all(0 < v[i] < 2 * n for i in range(3))]
    vd = Counter(val[v] for v in interior)
    return kinds, vd, len(tets)


def diagonal_planes(scheme, n=6):
    """Offsets (in cells) of the {110} and {111} facet planes n.x = d the uniform block carries."""
    tets = build(scheme, uniform(n, 2))
    fams = defaultdict(set)
    for t in tets:
        for f in itertools.combinations(t, 3):
            nv = cross(sub(f[1], f[0]), sub(f[2], f[0]))
            g = math.gcd(math.gcd(abs(nv[0]), abs(nv[1])), abs(nv[2]))
            nv = tuple(x // g for x in nv)
            if nv < tuple(-x for x in nv):
                nv = tuple(-x for x in nv)
            if sorted(abs(x) for x in nv) in ([0, 1, 1], [1, 1, 1]):
                fams[nv].add(dot(nv, f[0]) // 2)
    return {k: sorted(v) for k, v in sorted(fams.items())}


def plain_volumes(scheme):
    """Exact volumes (unit cube) of the tets one plain leaf emits, for both parities."""
    out = set()
    for origin in ((0, 0, 0), (2, 0, 0)):
        for t in plain_cell(scheme, origin, 2):
            out.add(Fraction(orient(t), 6 * 8))
    return sorted(out)


def main():
    for scheme in ("D", "X", "T5"):
        print(f"== scheme {scheme}")
        for name, (dmin, dmax, ar, n) in template_quality(scheme).items():
            print(f"  {name:32s} min {dmin:8.4f}  max {dmax:8.4f}  AR {ar:.4f}  ({n} tets)")
        for name, (leaves, lo, hi) in configurations().items():
            tets = build(scheme, leaves)
            bad, hang, vol = check_conformity(tets, lo, hi)
            print(f"  conformity {name:38s} tets {len(tets):5d}  bad faces {bad}  hanging {hang}  volume exact {vol}")
        kinds, vd, nt = isotropy(scheme)
        print(f"  uniform 6^3 block: {nt} tets; edge directions {dict(sorted(kinds.items(), key=str))}")
        print(f"  interior vertex valence {dict(sorted(vd.items()))}")
        print(f"  diagonal facet planes (normal: offsets in cells) {diagonal_planes(scheme)}")
        print(f"  plain-leaf tet volumes (unit cube, both parities) {[str(v) for v in plain_volumes(scheme)]}")


if __name__ == "__main__":
    main()
```

Output of the 2026-09-25 run (`python3 lattice_pattern.py`, ~2 s):

```text
== scheme D
  plain cell                       min  45.0000  max  90.0000  AR 1.3938  (48 tets)
  fan over a P face                min  45.0000  max 120.0000  AR 1.5607  (96 tets)
  fan over a Q quadrant triangle   min  35.2644  max 125.2644  AR 1.6052  (384 tets)
  fan over an E face (k=1)         min  45.0000  max  90.0000  AR 1.4268  (240 tets)
  conformity uniform 4^3                            tets   384  bad faces 0  hanging 0  volume exact True
  conformity one refined corner cell                tets   198  bad faces 0  hanging 0  volume exact True
  conformity two refined, face-adjacent             tets   252  bad faces 0  hanging 0  volume exact True
  conformity two refined, edge-only                 tets   276  bad faces 0  hanging 0  volume exact True
  conformity two refined, vertex-only               tets   288  bad faces 0  hanging 0  volume exact True
  conformity centre of 3^3 refined                  tets   492  bad faces 0  hanging 0  volume exact True
  conformity three levels, balanced                 tets  1914  bad faces 0  hanging 0  volume exact True
  conformity NEGATIVE: level 2 touching level 0     tets   642  bad faces 72  hanging 66  volume exact True
  uniform 6^3 block: 1296 tets; edge directions {'axis': 882, 'body(1, 1, 1)': 216, 'face(0, 1, 1)': 252, 'face(1, 0, 1)': 252, 'face(1, 1, 0)': 252}
  interior vertex valence {14: 125}
  diagonal facet planes (normal: offsets in cells) {(0, 1, -1): [-5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5], (1, -1, 0): [-5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5], (1, 0, -1): [-5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5]}
  plain-leaf tet volumes (unit cube, both parities) ['1/6']
== scheme X
  plain cell                       min  45.0000  max  90.0000  AR 1.3938  (48 tets)
  fan over a P face                min  45.0000  max 120.0000  AR 1.5607  (96 tets)
  fan over a Q quadrant triangle   min  45.0000  max  90.0000  AR 1.3938  (384 tets)
  fan over an E face (k=1)         min  45.0000  max  90.0000  AR 1.4268  (240 tets)
  conformity uniform 4^3                            tets   384  bad faces 0  hanging 0  volume exact True
  conformity one refined corner cell                tets   198  bad faces 0  hanging 0  volume exact True
  conformity two refined, face-adjacent             tets   252  bad faces 0  hanging 0  volume exact True
  conformity two refined, edge-only                 tets   276  bad faces 0  hanging 0  volume exact True
  conformity two refined, vertex-only               tets   288  bad faces 0  hanging 0  volume exact True
  conformity centre of 3^3 refined                  tets   492  bad faces 0  hanging 0  volume exact True
  conformity three levels, balanced                 tets  1914  bad faces 0  hanging 0  volume exact True
  conformity NEGATIVE: level 2 touching level 0     tets   642  bad faces 72  hanging 66  volume exact True
  uniform 6^3 block: 1296 tets; edge directions {'axis': 882, 'body(1, -1, -1)': 54, 'body(1, -1, 1)': 54, 'body(1, 1, -1)': 54, 'body(1, 1, 1)': 54, 'face(0, 1, -1)': 126, 'face(0, 1, 1)': 126, 'face(1, -1, 0)': 126, 'face(1, 0, -1)': 126, 'face(1, 0, 1)': 126, 'face(1, 1, 0)': 126}
  interior vertex valence {10: 90, 26: 35}
  diagonal facet planes (normal: offsets in cells) {(0, 1, -1): [-4, -2, 0, 2, 4], (0, 1, 1): [2, 4, 6, 8, 10], (1, -1, 0): [-4, -2, 0, 2, 4], (1, 0, -1): [-4, -2, 0, 2, 4], (1, 0, 1): [2, 4, 6, 8, 10], (1, 1, 0): [2, 4, 6, 8, 10]}
  plain-leaf tet volumes (unit cube, both parities) ['1/6']
== scheme T5
  plain cell                       min  54.7356  max  90.0000  AR 1.3660  (40 tets)
  fan over a P face                min  45.0000  max 120.0000  AR 1.5607  (96 tets)
  fan over a Q quadrant triangle   min  45.0000  max  90.0000  AR 1.3938  (384 tets)
  fan over an E face (k=1)         min  45.0000  max  90.0000  AR 1.4268  (240 tets)
  conformity uniform 4^3                            tets   320  bad faces 0  hanging 0  volume exact True
  conformity one refined corner cell                tets   189  bad faces 0  hanging 0  volume exact True
  conformity two refined, face-adjacent             tets   236  bad faces 0  hanging 0  volume exact True
  conformity two refined, edge-only                 tets   260  bad faces 0  hanging 0  volume exact True
  conformity two refined, vertex-only               tets   272  bad faces 0  hanging 0  volume exact True
  conformity centre of 3^3 refined                  tets   476  bad faces 0  hanging 0  volume exact True
  conformity three levels, balanced                 tets  1853  bad faces 0  hanging 0  volume exact True
  conformity NEGATIVE: level 2 touching level 0     tets   625  bad faces 72  hanging 66  volume exact True
  uniform 6^3 block: 1080 tets; edge directions {'axis': 882, 'face(0, 1, -1)': 126, 'face(0, 1, 1)': 126, 'face(1, -1, 0)': 126, 'face(1, 0, -1)': 126, 'face(1, 0, 1)': 126, 'face(1, 1, 0)': 126}
  interior vertex valence {6: 63, 18: 62}
  diagonal facet planes (normal: offsets in cells) {(1, -1, -1): [-10, -8, -6, -4, -2, 0, 2, 4], (1, -1, 1): [-4, -2, 0, 2, 4, 6, 8, 10], (1, 1, -1): [-4, -2, 0, 2, 4, 6, 8, 10], (1, 1, 1): [2, 4, 6, 8, 10, 12, 14, 16]}
  plain-leaf tet volumes (unit cube, both parities) ['1/6', '1/3']
```

## D.2 `reference_resolution.py` — what `run_reference.py` asks for, against the reference inputs

```python
#!/usr/bin/env python3
"""What resolution run_reference.py actually asks for, against the reference tool's own inputs.

For each reference case: the reference's background (*NUM_ELEMENT over *RVE_CORNERS) and
per-section *SAMR_LEVEL, the logged `Base Mesh Element size` and first `SAMR levels:` line
(what run_reference.py parses), and the ladder this mesher realises from them today (one root
cube on the longest axis; forest level = first halving <= h_max; finest = first halving <= h_min).
"""
import glob, math, os, re, sys

root = sys.argv[1] if len(sys.argv) > 1 else os.environ["RUSTMSPT_REFERENCE_DATASET"]
for case in ("TestCaseIntersect1", "TestCaseIntersect2", "TestCaseIntersect3"):
    d = os.path.join(root, case)
    log = open(sorted(glob.glob(os.path.join(d, "*_mesh.log")))[0], errors="replace").read()
    base = float(re.search(r"Base Mesh Element size:\s*([0-9.eE+-]+)", log).group(1))
    first_levels = int(re.search(r"SAMR levels:\s*(\d+)", log).group(1))
    corners = counts = None
    levels = []
    for f in sorted(glob.glob(os.path.join(d, "nurbs", "*.nurbs"))):
        lines = open(f, errors="replace").read().splitlines()
        for i, line in enumerate(lines):
            if line.startswith("*RVE_CORNERS") and corners is None:
                corners = [float(v) for v in lines[i + 1].split(",")]
            if line.startswith("*NUM_ELEMENT") and counts is None:
                counts = [int(v) for v in lines[i + 1].split(",")]
            if line.startswith("*SAMR_LEVEL"):
                levels.append(int(lines[i + 1].strip()))
    ext = [corners[i + 3] - corners[i] for i in range(3)]
    cell = [ext[i] / counts[i] for i in range(3)]
    longest = max(ext)
    # run_reference.py today: h_max = base, h_min = base / 2**max(first_levels, 1)
    h_max, h_min = base, base / 2 ** max(first_levels, 1)
    k = 0
    while longest / 2 ** k > h_max:
        k += 1
    K = 0
    while longest / 2 ** K > h_min:
        K += 1
    bg, fine = longest / 2 ** k, longest / 2 ** K
    ref_fine = cell[0] / 2 ** max(levels)
    print(f"{case}: reference background {counts} cells of {cell[0]:.6g} (cubic: {max(cell)/min(cell)-1:.1e}),"
          f" section levels {sorted(set(levels))} (per section {levels});"
          f" logged base {base:.6g} = sqrt2 x cell ({base/(math.sqrt(2)*cell[0]):.7f});"
          f" first 'SAMR levels' {first_levels}")
    print(f"    realised today: background {bg:.6g} ({[math.ceil(e/bg - 1e-9) for e in ext]} cells, {bg/cell[0]:.4f} x reference),"
          f" finest {fine:.6g} ({fine/ref_fine:.4f} x the reference's finest {ref_fine:.6g}), levels {K-k}")
```

Output of the 2026-09-25 run (`RUSTMSPT_REFERENCE_DATASET` at the dataset root):

```text
TestCaseIntersect1: reference background [17, 17, 17] cells of 26.4706 (cubic: 0.0e+00), section levels [2] (per section [2, 2]); logged base 37.4351 = sqrt2 x cell (1.0000009); first 'SAMR levels' 2
    realised today: background 28.125 ([16, 16, 16] cells, 1.0625 x reference), finest 7.03125 (1.0625 x the reference's finest 6.61765), levels 2
TestCaseIntersect2: reference background [24, 24, 30] cells of 0.0166667 (cubic: 4.4e-16), section levels [2] (per section [2, 2]); logged base 0.0235702 = sqrt2 x cell (0.9999989); first 'SAMR levels' 2
    realised today: background 0.015625 ([26, 26, 32] cells, 0.9375 x reference), finest 0.00390625 (0.9375 x the reference's finest 0.00416667), levels 2
TestCaseIntersect3: reference background [22, 45, 45] cells of 0.0614125 (cubic: 3.1e-12), section levels [0, 2] (per section [2, 2, 2, 2, 2, 2, 2, 2, 0]); logged base 0.0868504 = sqrt2 x cell (1.0000001); first 'SAMR levels' 0
    realised today: background 0.0863613 ([16, 32, 32] cells, 1.4062 x reference), finest 0.0431807 (2.8125 x the reference's finest 0.0153531), levels 1
```
