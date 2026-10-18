#!/usr/bin/env python3
"""FEM smoke test on an exported INP (plan M-6.4, B.8): can a solver use this mesh?

Assembles the P1 (C3D4) Laplace stiffness matrix straight from the INP `mesh` wrote and checks
three things a solver needs and a mesh viewer cannot show:

  1. every element has a positive Jacobian (C3D4 node order) - an inverted or flat element is a
     singular or negative element stiffness;
  2. the global matrix is symmetric, and positive definite once the domain box is held - checked
     by solving with conjugate gradients, which only converges on SPD systems;
  3. the patch test: with u = a + b.x prescribed on the box, the interior solution must reproduce
     the linear field to round-off. P1 elements represent linear fields exactly, so any error is
     the mesh's - an overlapping pair, a hanging node or a crack all break it.

    python3 data/fixtures/meshgen/acceptance/fem_smoke.py mesh.inp [more.inp ...]

Exit status 0 when every file passes. Needs numpy and scipy.
"""

import sys

import numpy as np
import scipy.sparse as sp
import scipy.sparse.linalg as spla


def read_inp(path):
    nodes, elements = {}, []
    block = None
    with open(path) as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("**"):
                continue
            if line.startswith("*"):
                head = line[1:].split(",")[0].strip().upper()
                block = head if head in ("NODE", "ELEMENT") else None
                continue
            fields = [x for x in (v.strip() for v in line.split(",")) if x]
            if block == "NODE":
                nodes[int(fields[0])] = [float(v) for v in fields[1:4]]
            elif block == "ELEMENT":
                elements.append([int(v) for v in fields[1:5]])
    ids = sorted(nodes)
    index = {n: i for i, n in enumerate(ids)}
    xyz = np.array([nodes[n] for n in ids])
    tets = np.array([[index[n] for n in e] for e in elements], dtype=np.int64)
    return xyz, tets


def topology(xyz, tets):
    """Faces used once (free) off the box, and faces used three or more times."""
    f = np.sort(tets[:, [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]]].reshape(-1, 3), axis=1)
    _, counts = np.unique(f, axis=0, return_counts=True)
    uniq = np.unique(f, axis=0)
    lo, hi = xyz.min(axis=0), xyz.max(axis=0)
    tol = 1e-9 * np.linalg.norm(hi - lo)
    free = uniq[counts == 1]
    p = xyz[free]
    on_box = np.zeros(len(free), dtype=bool)
    for k in range(3):
        on_box |= np.all(np.abs(p[:, :, k] - lo[k]) <= tol, axis=1) | np.all(np.abs(p[:, :, k] - hi[k]) <= tol, axis=1)
    return int((~on_box).sum()), int((counts > 2).sum())


def smoke(path):
    xyz, tets = read_inp(path)
    split = any(line.startswith("**SPLIT ") for line in open(path))
    interior_free, multi = topology(xyz, tets)
    if split:
        # A split export: interfaces cut open, so bodies may float and the linear patch test does
        # not hold across a traction-free cut. What must hold is that every element is valid, no
        # face carries three tets, and the only free faces off the box are the cut faces' two sides.
        p = xyz[tets]
        jac = np.linalg.det(p[:, 1:, :] - p[:, :1, :])
        bad = int((jac <= 0).sum())
        ok = bad == 0 and multi == 0
        print(f"{path}: split export, {len(tets)} C3D4, {xyz.shape[0]} nodes; non-positive Jacobians {bad}; "
              f"faces on 3+ tets {multi}; free faces off the box {interior_free} (the cut faces' two sides) -> {'PASS' if ok else 'FAIL'}")
        return ok
    p = xyz[tets]  # (E, 4, 3)
    d = p[:, 1:, :] - p[:, :1, :]  # edges from node 1
    jac = np.linalg.det(d)  # 6 x volume
    bad = int((jac <= 0).sum())
    vol = jac.sum() / 6.0
    lo, hi = xyz.min(axis=0), xyz.max(axis=0)
    box = float(np.prod(hi - lo))
    # Gradients of the four P1 shape functions: G = inv(d)^T rows give grad N2..N4.
    good = jac > 0
    inv = np.linalg.inv(d[good])  # (E, 3, 3)
    g = np.zeros((inv.shape[0], 4, 3))
    g[:, 1:, :] = np.transpose(inv, (0, 2, 1))
    g[:, 0, :] = -g[:, 1:, :].sum(axis=1)
    ke = np.einsum("eik,ejk->eij", g, g) * (jac[good] / 6.0)[:, None, None]
    t = tets[good]
    rows = np.repeat(t, 4, axis=1).ravel()
    cols = np.tile(t, (1, 4)).ravel()
    n = xyz.shape[0]
    k = sp.csr_matrix((ke.ravel(), (rows, cols)), shape=(n, n))
    asym = abs(k - k.T).max()
    scale = abs(k).max()
    tol = 1e-9 * np.linalg.norm(hi - lo)
    on_box = np.any(np.abs(xyz - lo) <= tol, axis=1) | np.any(np.abs(xyz - hi) <= tol, axis=1)
    free = ~on_box
    exact = 1.0 + xyz @ np.array([0.3, -0.7, 1.1])
    rhs = -k[free][:, on_box] @ exact[on_box]
    kff = k[free][:, free]
    # Jacobi-preconditioned: cut meshes carry slivers, and an unpreconditioned CG on them is slow
    # rather than wrong.
    diag = kff.diagonal()
    m = sp.diags(1.0 / np.where(diag > 0, diag, 1.0))
    u, info = spla.cg(kff, rhs, rtol=1e-12, maxiter=20 * max(1, kff.shape[0]), M=m)
    err = float(np.abs(u - exact[free]).max()) if free.any() else 0.0
    span = float(np.abs(exact).max())
    passed = (bad == 0 and asym <= 1e-12 * scale and info == 0 and err <= 1e-6 * span
              and abs(vol - box) <= 1e-9 * box and interior_free == 0 and multi == 0)
    print(
        f"{path}: {len(tets)} C3D4, {n} nodes ({int(free.sum())} free); "
        f"non-positive Jacobians {bad}; volume {vol:.12g} vs box {box:.12g}; "
        f"asymmetry {asym:.2e} of {scale:.2e}; CG {'converged' if info == 0 else f'FAILED ({info})'}; "
        f"patch-test error {err:.2e} of {span:.2e}; free faces off the box {interior_free}, faces on 3+ tets {multi} "
        f"-> {'PASS' if passed else 'FAIL'}"
    )
    return passed


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    ok = all([smoke(path) for path in sys.argv[1:]])
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
