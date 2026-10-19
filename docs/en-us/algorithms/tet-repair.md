# Tetrahedral Mesh Repair

The `tet-repair` subcommand repairs the cells of a tetrahedral mesh that a solver
cannot take — inverted cells and cells over an aspect-ratio limit — by flips and
node moves, without changing which material any part of the mesh belongs to. It
is simHelper's `meshing/tet_repair.flip_repair` (its closure repairs) as native
code: on a closure mesh of 1,152,074 cells it takes 0.24 s for a repair that takes
minutes in Python, most of which went on building per-node sets and evaluating
one cell's quality at a time.

## Quality

The aspect ratio is YAFEQ's (`MESH_verification`): circumradius over three times
the inradius, 1 for a regular tetrahedron, the circumradius from
`|u|²(v×w) + |v|²(w×u) + |w|²(u×v)| / 12|V|` with `u, v, w` the edges to the fourth
corner. A cell with `|V| <= 1e-30` has an infinite ratio; a cell is *bad* when its
volume is not positive or its ratio is over `limit`.

## The rule

1. **Padding.** With `padding` given, a bad cell of no volume (`|V| <= 1e-9 L³`, `L`
   its longest edge) with a face on a padding cell and every corner on a padding cell
   is given to the padding. A flat cell with a corner inside the body stays: given
   away, it would leave a slit of no thickness in the body's outer surface, down to
   that corner.
2. **Flips**, worst watched cell first, in sweeps (at most `sweeps`): 2-3 through a
   face shared with one cell of the same density, 3-2 round an edge of three cells,
   4-4 round an edge of four (both diagonals tried). A flip is taken when every new
   cell is positive, the replaced cells' signed volume is kept (to `1e-9` relative),
   and the worst new ratio is below the worst old one; the lowest worst new ratio
   wins. Never between densities, never removing a face whose nodes are all fixed,
   an edge whose two nodes are fixed, or a face of the edge with a ring node that is
   all fixed.
3. **Moves.** A watched cell still bad moves one of its nodes to where the cells
   round it are all positive and their total excess over `limit` is least —
   Nelder-Mead from the node and from the mean of its neighbours, twice each
   (`xatol`/`fatol` `1e-6`, then `1e-7`) — if that is less than now. A free node (one
   density round it) moves anywhere; with `wall_tol > 0` a node between two
   densities moves only within `wall_tol` of the faces between them, and a fixed
   node on `layer` faces within `wall_tol` of those. Nodes on the bounding box stay.
4. Flips and moves alternate until neither helps, at most four times.
5. **Joint pass.** Cells still inverted, grouped by shared nodes, move all their
   movable nodes at once (a node between two densities along its wall, a layer node
   along its layer, a layer node on a wall along both, within `joint_tol`): a random
   search of 20,000 draws from a ChaCha8 generator seeded with `seed`, its step
   shrinking from 0.4 of the group's mean edge, then Nelder-Mead; taken when no cell
   round them is inverted and the score improves. Then flips and moves again.

Nelder-Mead is scipy's, non-adaptive (reflection 1, expansion 2, contraction and
shrink 0.5; the initial simplex steps each coordinate by 5 %, a zero one by
0.00025). On the closure mesh above every flip and every Nelder-Mead move is the
one simHelper's Python repair takes, to the reported four places.

## What differs from simHelper's Python

Two things, both choices among equally valid repairs, never a weaker result:

- **Order among ties.** Watched cells are visited worst first; cells of equal
  ratio (all inverted cells tie at infinity) go in index order here and in Python
  set order there.
- **The joint pass's draws** come from a seeded ChaCha8 generator here and from
  numpy's there, so its random search lands elsewhere. On the closure mesh the two
  nodes it moved ended 0.06 and 0.37 µm from Python's, both with no cell inverted
  and none over the limit.

A run is deterministic for a given seed and independent of the worker count.

## Parallelism

The repair itself is sequential — each flip or move changes what the next one
sees, and the bad cells are a handful — so its cost is in the native arithmetic,
not in threads. The full-mesh passes (the first quality of every cell, the search
for inverted cells, the final counts) run on rayon in the run's own pool
(`cpu_max`). There is no GPU path: the work that remains after the native rewrite
is a few thousand local evaluations, below what a dispatch costs.

## Inputs and outputs

See [`examples/tet-repair.md`](../examples/tet-repair.md) for a run and
[`reference/pipeline-tet-repair.md`](../reference/pipeline-tet-repair.md) for the
functions. Arrays travel as `.npy` files: `points` `(n, 3)` f64; `tets` `(m, 4)`,
`density` `(m)`, `fixed` `(n)` (0/1) non-negative integers; optional `layer`
`(k, 3)`. The outputs are `points.npy`, `tets.npy` (int64, the live cells in
order: the input's survivors, then the cells flips made), `density.npy` (int64)
and `report.json` (`method_version` `tet-repair-v1`, the build identity, the
parameters, counts, the repair report — cells bad before, cells given to the
padding, each flip, each node moved, cells inverted and over the limit after, the
worst ratio after, the padding not counted — and wall times).
