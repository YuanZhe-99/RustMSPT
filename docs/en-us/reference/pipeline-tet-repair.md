# Tetrahedral Mesh Repair — `tet-repair`

Reference for the `tet-repair` subcommand: the quality measure, Nelder-Mead and the
repair (`src/geometry/tet_repair.rs`), the pipeline (`src/pipeline/tet_repair.rs`) and
its configuration (`src/config/tet_repair.rs`). The `.npy` I/O is `state-map`'s
([`pipeline-state-map.md`](pipeline-state-map.md)); the point-to-triangle distance is
`closest`'s ([`pipeline-closest.md`](pipeline-closest.md)). The algorithm is
[`algorithms/tet-repair.md`](../algorithms/tet-repair.md).

## Index

| Item | Source | Summary |
|---|---|---|
| `FLAT` | `src/geometry/tet_repair.rs:7` | `1e-30`: a cell with no more volume has an infinite ratio. |
| `RepairParams` | `src/geometry/tet_repair.rs:13` | limit, sweeps, wall_tol, padding, joint_tol, seed. |
| `Flip` | `src/geometry/tet_repair.rs:24` | A flip taken: kind, where, worst ratio before and after. |
| `Moved` | `src/geometry/tet_repair.rs:32` | A node moved: node, along a wall or a layer, distance, excess before (`None`: inverted) and after, joint pass or not. |
| `ToPadding` | `src/geometry/tet_repair.rs:43` | A flat cell given to the padding. |
| `RepairReport` | `src/geometry/tet_repair.rs:51` | Bad before, to padding, flips, moves, inverted and over the limit after, worst ratio after. |
| `Repaired` | `src/geometry/tet_repair.rs:62` | Points, live cells, densities, report. |
| `aspect_ratio` | `src/geometry/tet_repair.rs:95` | YAFEQ's ratio and the signed volume of one cell. |
| `nelder_mead` | `src/geometry/tet_repair.rs:136` | scipy's non-adaptive Nelder-Mead. |
| `repair` | `src/geometry/tet_repair.rs:692` | The whole rule: padding, flips and moves, the joint pass. |
| `TetRepairParams` | `src/config/tet_repair.rs:9` | The `tet_repair` YAML block. |
| `TetRepairConfig` | `src/config/tet_repair.rs:45` | The document. |
| `METHOD_VERSION` | `src/pipeline/tet_repair.rs:9` | `tet-repair-v1`, written in every report. |
| `TetRepairPipeline` | `src/pipeline/tet_repair.rs:11` | The subcommand's pipeline. |
| `TetRepairPipeline::run` | `src/pipeline/tet_repair.rs:22` | Read, repair, write the mesh and the report. |

## `src/geometry/tet_repair.rs`

### `aspect_ratio(q: &[[f64; 3]; 4]) -> (f64, f64)`
- **Purpose:** YAFEQ's aspect ratio and the signed volume of one cell.
- **Returns:** `(ratio, volume)`; the ratio is infinite when `|volume| <= FLAT`.
- **Side effects:** none.
- **Notes:** simHelper's `yafeq_aspect_ratio` and YAFEQ's `MESH_verification` use the same formula.

### `nelder_mead(f, x0, xatol, fatol, maxiter) -> (Vec<f64>, f64)`
- **Purpose:** minimise `f` from `x0` as scipy's `minimize(method="Nelder-Mead")` does without `adaptive`.
- **Returns:** the best vertex and its value.
- **Side effects:** calls `f`.
- **Notes:** initial simplex: each coordinate stepped by 5 % (0.00025 when zero); stops when every vertex is within `xatol` of the best and every value within `fatol`, or after `maxiter` iterations.

### `repair(points, tets, density, fixed, layer, prm) -> Result<Repaired>`
- **Purpose:** repair inverted cells and cells over `prm.limit` by the rule in the algorithm page.
- **Parameters:** `points` flat xyz; `tets` flat, four per cell; `density` per cell; `fixed` per node (a cohesive layer's nodes); `layer` flat node triples (faces a fixed node may slide along, with `wall_tol > 0`); `prm`.
- **Returns:** `Repaired`, or `InvalidConfig` for ragged arrays or a node index out of range.
- **Side effects:** none.
- **Notes:** deterministic for a seed; ties in visiting order go to the lower index. Never flips between densities or through fixed faces.

## `src/pipeline/tet_repair.rs`

### `TetRepairPipeline::run(&self) -> Result<()>`
- **Purpose:** the subcommand.
- **Parameters:** `self.config.tet_repair` — the array paths, `output_dir`, `limit` (default 10), `sweeps` (20), `wall_tol` (0), `padding`, `joint_tol`, `seed` (0), `cpu_max`.
- **Returns:** `Ok`, or an error for an unreadable or ragged array, an index out of range, or an I/O failure.
- **Side effects:** reads the arrays; writes `points.npy`, `tets.npy`, `density.npy` and `report.json` under `output_dir`; prints one summary line.
