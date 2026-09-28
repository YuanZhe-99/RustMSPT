# HANDOFF — mesh generation (`mesh` subcommand), for the next session

This file is public. It carries no host names, remote URLs, personal paths, account names, e-mail
addresses or private links. Where the next session needs one of those, it says where to find it.

## 1. What this work is

RustMSPT's `mesh` subcommand is being built stage by stage against three frozen specs
(`SPEC_meshgen_geometry.md`, `SPEC_meshgen_numerics.md`, `SPEC_meshgen_contracts.md`) and one plan,
`PLAN_mesh_generation.md`. **The plan is git-excluded** (`.git/info/exclude`): it lives on disk,
must be staged with `git add -f PLAN_mesh_generation.md`, and a fresh clone does not have it — copy
it across when migrating the environment. Its §11 "Subtask rollup" table has a `Status (date)`
column (● landed, ◐ partly landed with what is open named, ○ not started); that table is the
progress summary and must be updated with every landed or re-measured step.

Two S8 paths exist. The **default** path is what ships today; the **gated** path
(`RUSTMSPT_PLC_PASS=1`, the local PLC mesher of SPEC geometry §7) is the one plan M-3 makes the only
path. Both are measured on a nine-case acceptance matrix (a1 … a8) and three reference cases.

## 2. Standing instructions from the owner

- **Reply in Chinese.** End every substantial reply with the four-part status block defined in
  `AGENTS.md` ("Reporting to the owner"): what finished, what is next, where things stand, what is
  needed from the owner.
- **Autonomy:** the owner has authorised continuing the plan to completion without asking,
  self-checking against the criteria below, and stopping only where a decision or a visual judgement
  genuinely needs the owner.
- **No correctness knobs** (R3): a feature decides per cell; a behaviour-changing env var is a
  diagnosis, not a design. Print-only diagnostic env vars are fine.
- **Land a change only when it measures strictly better** on the goal properties (the matrix,
  both paths where affected); record refuted attempts in the plan with their numbers.
  Conformity (`[V1]`/`[V3]`) is never traded.
- **Every code change** updates, in the same commit: `docs/en-us/**` and the mirrored
  `docs/zh-cn/**` (structurally identical), `docs/*/reference/function-index.md` rows for new
  functions, `AGENTS.md` (conventions, pitfalls, lessons), and the plan (status block + §11 row).
- **R-N1:** never name the external reference mesher anywhere in the repo, in commits, or in
  reports; cite it by role ("the reference implementation", "the reference dataset").
- **Commits** go on `main`, end with the co-author trailer the harness supplies, and must not stage
  `data/fixtures/meshgen/acceptance/__pycache__/`. A release is a tag pushed to both remotes (see
  `git remote -v`); do not push unless asked.
- **R-P2:** every change to meshing must stay byte-identical across thread counts
  (`check_determinism.py`, below).

## 3. Visual verification and the report Artifact (owner requirements)

- **R9 — every visual check sets the input STL beside the output, from the same camera, with the
  output's material boundary coloured by deviation from the input** (grey < 2 %, yellow 2–10 %,
  orange 10–25 %, red ≥ 25 % of the local edge). Defects are located (coordinates, component),
  clustered, ranked by off-surface area and shown head-on and oblique. A picture of the mesh alone
  (region-coloured sections, wireframes) was rejected as showing nothing. Tool:
  `data/fixtures/meshgen/acceptance/render_focus.py` (uses `mesh-verify … fidelity_vtu:`).
- **Look at the images yourself before reporting** a change that alters the mesh; say what the
  picture shows, not only the numbers.
- **The owner works under WSL and reads results through a claude.ai Artifact link**, not local
  image files. There is **one** report Artifact, titled **"网格检查点（M-1.0 起，持续更新）"**, written
  in Chinese. **Always republish to that same Artifact; never create a new one.** Find it with the
  Artifact tool's `list` action (match the title) — its URL is intentionally not written here.
- **Updating it from a new session:** its HTML source lived in a session-scratch directory that
  does not survive the migration. Use the Artifact tool's `read` action on its URL to get the
  current HTML back, edit that file (new sections go at the top, directly under the "怎么看"
  legend section; older sections stay), and publish with `url` set to the same Artifact. Images
  already on the page are hosted with it; new images are added through the publish call's `files`
  map (`img/<name>.jpg`, JPEG, cropped to content, ~600–1200 px). The page defines a `.grid2`
  figure grid and a click-to-enlarge lightbox that the new sections reuse.
- Each report section says, in Chinese: what was found (mechanism, not just numbers), what changed,
  a before/after table, and before/after comparison images from `render_focus.py`.

## 4. Where things stand (end of 2026-09-28)

Matrix, `[V13]` on-surface share of the material boundary (P3's corner measure), gated path
(`data/output/acceptance_m34`, the last committed code; `data/output` is not in git):

| case | a1 | a2 | a3 | a4 | a6a | a6b | a7a | a7b | a8 |
|---|---|---|---|---|---|---|---|---|---|
| gated on % | 95.911 | 100.000 | 98.723 | 99.482 | 99.953 | 99.904 | 99.772 | 99.778 | 98.897 |
| default on % | 93.977 | 100.000 | 93.767 | 97.570 | 99.469 | 99.248 | 98.723 | 99.163 | 91.869 |

- **Checks:** on both paths the nine cases fail only `[V13]` (all but a2) and `[V6]` on a3.
  `[V1]`, `[V2]`, `[V3]`, `[V9]`, `[V12]` pass on all nine, both paths.
- **Reference cases** (gated / default FAIL sets): case 1 {V6, V13} / {V6, V13}, case 2 {V6, V13} /
  {V6, V13}, case 3 {V9, V13} / {V13}. Gated case 1 on-surface 94.96 % against default 95.25 %; gated element counts 0.96× / 0.74× / 1.34× the reference tool's, at its own resolution.
- **M-3.1** (gated no worse than default on any FAIL gate, better on `[V13]`): holds on the nine;
  **open** on reference case 3 (`[V9]`, one node) and reference case 1 (`[V13]`, 0.3 points).
- **Done this session:** M-1.2 (R-P2 on all twelve, both paths), M-1.3 (time budget line),
  M-1.4 (P2 at equal fidelity), M-1.8 (reference cases at their own resolution); M-1.6 partly
  (S2 keeps constructed points on their facets); M-2.1 partly (see the plan).
- **Blocked:** M-2.4 (`contact_chamfered_by` does necessary work while a6a/a6b's contact strip is
  thinner than one lattice face).

The plan's §7 status blocks and §11 table are the authoritative, detailed record — read them first.

## 5. Next steps, in order

1. The remaining over-covered facets on reference case 1: the "interior not covered" class is
   almost entirely flat tets that `remove_flat_quad_tets` cannot yet remove (379 of 416 refused
   facets have some; `RUSTMSPT_FACET_DIAG` prints `flat N`, `RUSTMSPT_PLC_CELL=<i>` prints
   `[FLAT-SKIP]` with each skipped tet's neighbour apexes). Closing it should also close the
   0.3-point `[V13]` gap to the default path on that case.
2. The "link polygon has no valid triangulation" class (largest stranded class on most cases).
3. M-2.3: remove the whole-cell fan; it carries reference case 3's `[V9]` node and a3's `[V6]`.
4. Then M-3 (one path), M-4 … M-8 as the plan orders them.

## 6. How to build, run and measure

- Build into a separate target dir so a running matrix is never disturbed, and copy the binary
  before starting a long run:
  `CARGO_TARGET_DIR=target/dev cargo build --release`, then point `RUSTMSPT_BIN` at a copy.
- Matrix: `python3 data/fixtures/meshgen/acceptance/run_acceptance.py --path gated|default
  --json <work>/<path>.json` with `RUSTMSPT_ACCEPTANCE_WORK=<work>`. Add `--focus` for renders
  (slow on a8). Compare new work dirs against the previous ones side by side before landing.
- Reference cases: `run_reference.py`; the dataset is outside the repo, set
  `RUSTMSPT_REFERENCE_DATASET`; the path follows `RUSTMSPT_PLC_PASS` in the environment.
- Determinism: `check_determinism.py <work> --path P --out table`.
- P2 sweep: `p2_equal_fidelity.py <work> --out table`.
- One cell, one node: mesh with `RUSTMSPT_CUT_DIAG=1` (adds `parent_cell`, `plc_path`,
  `escalation_reason` to the contract VTU), then `node_dump.py <contract.vtu> <node>`;
  `RUSTMSPT_PLC_CSV=<file>` for the per-cell §7.4 census and `RUSTMSPT_PLC_CELL=<index>` to turn the
  cdt dumps on for that one cell.
- Python helpers that need numpy or Pillow are run with `uv run --with numpy|pillow python …`.
- All of the above is documented in `docs/en-us/reference/mesh-verify.md` → "Acceptance tooling".

## 7. Lessons from this session worth reading before touching S2/S8

In `AGENTS.md` §10 (latest entries at the end of the list). The ones that most shape the next
steps: an identity key must never become a coordinate (S2); when one stage learns a face is not a
boundary, every consumer must learn it; a per-face "strictly interior" test with a relative slack
admits one point for every face around an edge; a verifier claim must be one the mesher can truly
make (curve carriage at the duplicate bound, not at `eps`); a mesh repair that overwrites tets in
place must refresh every record pointing at them.

## 8. State of the working tree at handoff

Everything is committed on `main` and pushed to both remotes; `git status` shows only the untracked
`__pycache__` directory. Measurement outputs under `data/output/` are not in git and are
reproducible with the scripts above. The plan file must be copied separately (it is git-excluded).
