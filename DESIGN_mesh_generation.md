# Mesh generation — design of record

**Status: this document supersedes `PLAN_mesh_generation.md` on architecture.** The PLAN remains
the measurement log — its recorded numbers, defect diagnoses and gate history are hard-won and
stay authoritative *as records*. Where the PLAN describes what the mesher should be, this
document wins.

Written 2026-08-14, after an audit found the implementation had drifted from the goal in a way
that fifteen recorded "passes" of patching had not addressed, because each patch treated a
symptom of one architectural decision.

---

## 1. The goal

Generate a tetrahedral volume mesh from **any** STL input, with:

| # | property | normative meaning |
|---|---|---|
| **P1** | any input | any STL set the user supplies: intersecting, touching, nested, sheets, defective. No input class is out of scope, and no input causes a hard failure |
| **P2** | minimum elements | the fewest elements that satisfy P3 and P4. Element count is a first-class output, not a byproduct |
| **P3** | exact surfaces | every material boundary in the mesh **lies on the input surface**. Not "within `h`", not "bounded chamfer" — on it |
| **P4** | no bad elements | every element is usable by an implicit FEM solver, on the quality gates the contract defines |

These are conjunctive. A mesh that meets three of four does not meet the goal.

### What P3 forbids

P3 is the property the rest of this document turns on, so state it precisely. A material
boundary is a face between two elements whose region sets differ, or between an element and the
void. **Every such face must lie in the input surface.** A face that is parallel to the surface
and a fraction of `h` away from it is a violation, however small the fraction. Visible serration
is the symptom; the violation is the displacement itself.

This is not an aspiration bolted on late. It is the defining property of the method family this
mesher belongs to — *conforming to the interface*. A mesh that does not conform to the interface
is a different, weaker method wearing the same name.

---

## 2. Current state, audited

Measured 2026-08-14 on the nine acceptance cases and on the shared reference dataset.

| property | verdict | evidence |
|---|---|---|
| **P1** any input | **partial** | the nine cases and the reference dataset run end to end. Each new input class has historically required a fix (radial-patch valence, K1 double crossings, coplanar strut lattices), which is the signature of enumerated cases rather than a general method |
| **P2** minimum elements | **FAIL, 3.6×** | on the same three STLs, this mesher produces **1,269,546** tets where the reference produces **354,372** |
| **P3** exact surfaces | **FAIL** | escalated cells are coned to a Steiner point at the cell centroid; their material boundary is a staircase of fan faces that were never placed on the surface |
| **P4** no bad elements | **FAIL** | `[V4]` warns on all nine cases |

### The single root cause

All three failures descend from one decision. Gate **G6-0** evaluated a local-PLC mesher for
cells the frozen §6 cut table cannot express, judged it not implementable deterministically at
cell scale, and adopted a **conforming centroid fan** as the fallback: cone the cell to a Steiner
point at its centroid, then settle each fan tet's ownership by sampling its own centroid.

The fan is unconditionally valid, and that is genuinely why the mesher is watertight and passes
`[V1]`/`[V3]`/`[V9]` everywhere. But:

- it **abandons P3 by construction** — it does not cut along the surface at all, so the material
  boundary inside every escalated cell is off-surface by up to `h`;
- it **costs P2** — a fanned cell emits one tet per boundary triangle instead of the two to six a
  conforming cut would produce, and escalated cells are not rare;
- it **costs P4** — fan tets are coned to a centroid regardless of shape, which is a sliver
  factory wherever the cell is thin or the piece is re-entrant.

Everything since G6-0 has been an attempt to recover P3 while keeping the fan. The record is
unambiguous that this cannot work: four scoped variants of the ownership check, naming the
`on_cut` surface in the split, 2.5× refinement, and finally the spoke cut — which cut fan tets on
a *plane* through three crossings, moved the declared-boundary metric 26 %, left the serration
visually intact, and made one fixture's plate 2.6× thinner-than-true. It was reverted.

**The fallback is the drift.** No refinement of §7.6 reaches P3, because §7.6's premise is that
the surface is not represented inside the cell.

---

## 3. Required architecture

### R1 — No fallback may abandon conformity to the interface

Every cell the surface passes through must be subdivided so that the surface is a **union of
element faces**. A cell the frozen table cannot express is a gap in the table or in the
preceding stages, and must be closed there — not diverted to a construction that gives up the
defining property.

This is the decision G6-0 made the other way, and it is re-opened. The determinism objection
that decided it is real and must be answered, not ignored: the subdivision of a cell must be a
pure function of that cell's own geometry and its shared faces, so that two cells sharing a face
derive the same triangulation of it without negotiating. That is the same constraint §5.2's face
table and §4's SNK rule already satisfy, and it is the constraint any replacement inherits.

### R2 — Element count is a gate, not an outcome

P2 needs a number to steer on. The mesher must report elements-per-unit-surface-area and total
count against the reference on the shared dataset, and a change that raises the count without a
P3/P4 justification is a regression.

The 3.6× gap decomposes into at least: the fan's per-boundary-triangle emission, an octree that
refines toward curves it cannot then represent (measured: refining A-6a 2.5× moved curve coverage
1 of 35 → 0 of 35, i.e. the refinement bought nothing), and sizing driven by criteria that do not
correspond to what the cut can capture. **Refinement that cannot be represented in the output is
pure cost** and must be removed rather than tuned.

### R3 — Features adapt; there are no correctness knobs

Every stage decides for itself, per cell, what produces the correct highest-quality result.
There is no setting that chooses between a correct mesh and an incorrect one. Reaching for a
behaviour flag is a diagnosis: either the stage lacks its adaptive criterion, or the architecture
cannot reach the goal. Diagnostic env vars that change only what is *printed* are exempt.

### R4 — The delivered mesh is the volume

The deliverable is a tets-only unstructured grid carrying region identity as a **cell array**.
Interface geometry is implied by the region array on the two elements sharing a face; it is not
re-encoded as separate triangle cells in the delivered file. The face-tag contract that later
stages need (cohesive/split-node data, rim curves) lives in an auxiliary file alongside.

The reference demonstrates the standard: its output is 354,372 cells, every one a tetrahedron,
carrying one material cell array. Opened directly, its only feature edges are the domain box.

### R5 — Acceptance is measured on the surface, not on proxies

P3 is a statement about displacement, so it is measured as displacement: area-weighted distance
from each component's meshed material boundary to its input surface, reported per case, with the
maximum bounded.

This is a direct consequence of a failure recorded on 2026-08-14: a metric that counted
*undeclared* boundary faces improved 26 % while the surface itself did not move on one fixture and
got worse on another. Proxy metrics may inform; they may not gate. `[V5]`'s `abs_distance_max` is
also not this measure — it compares *declared interface faces* to the input surface, so it reads
essentially exact while the undeclared staircase beside it is the actual defect.

---

## 4. What changes, in order

**Phase 1 — measure the goal.** Implement the P3 fidelity check (R5) and the P2 element-count
gate (R2), and record all four properties on the nine cases and the reference dataset. Nothing
downstream can be judged until the goal is measurable. Neither depends on the meshing change and
both can land immediately.

**Phase 2 — deliver the volume.** Make the tets-only volume the delivered mesh (R4), the
mixed-cell file auxiliary. Contained, independent of everything else, closes one goal property
outright.

**Phase 3 — re-open G6-0.** Design the conforming subdivision that replaces the fan (R1), with
determinism as the property to prove rather than the reason not to try. This is a prototype gate
with a GO/NO-GO, and its acceptance is P3 on the nine cases plus the reference dataset.

**Phase 4 — re-derive sizing against what the cut can represent.** Once the cut can carry the
surface, the sizing field's job changes: it refines to resolve features the cut will actually
capture, not features it merely detects. P2 is judged here.

**Phase 5 — quality.** The existing G8 content, run against the final element population. It was
correctly blocked while that population was still going to change.

---

## 5. What is kept

The audit is about architecture, not about the work. These stand and must not be re-derived:

- **The exact-predicate foundation.** `orient3d` with the static filter and exact fallback, the
  frozen ray-direction sequence, the degeneracy ladder. Every robustness property rests on this.
- **S0–S2.** Welding, repair, feature detection, the arrangement with its coplanar overlay and
  coincidence policy, topology rebuild and GWN fallback. These are input conditioning and are
  independent of how cells are cut.
- **S3.** The gap field, regime segmentation and mid-surface validation.
- **The frozen tables.** §4's SNK rule, §4.3's prism patterns, §5.2's face split, §6's cut table.
  These are correct where they apply; the fault is what happens when they do not.
- **The verifier.** `[V1]`–`[V12]`, the corrupted-fixture manifest, and the discipline of
  asserting the exact code set each fixture fires.
- **The measurement record.** `PLAN_mesh_generation.md`'s recorded defects and their diagnoses,
  and the pitfalls in `AGENTS.md`. Every one was paid for.

---

## 6. Invariants that survive any rewrite

- **J1.** A face shared by two cells is triangulated identically by both, derived from the face
  alone without negotiation.
- **K1/K2.** An edge crossed more than once, and cut-node identity per (edge, component).
- **B1.** A thin region's two walls collapse at the cut node, not at the element.
- **R-P2.** Two runs of the same input produce byte-identical output.
- **Sheets never claim volume.**
