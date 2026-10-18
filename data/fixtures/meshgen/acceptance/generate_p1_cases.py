#!/usr/bin/env python3
"""Fixtures for the P1 campaign (plan M-4.4, B.9): the cases the suite never built.

    python3 data/fixtures/meshgen/acceptance/generate_p1_cases.py

Writes, beside this script:
  a5_cube.stl, a5_sphere.stl        A-5   sphere inside cube (container/contained symmetry)
  a11_sheet.stl                     A-11  an open tilted sheet crossing the whole domain
  a13_sphere_defective.stl          A-13  a sphere with a duplicated triangle, a flipped one and
                                          a vertex split 1e-6 apart (a sub-eps gap)
  a14a_two_cubes.stl                A-14a two disjoint closed cubes in ONE file
  a14b_cube_and_quad.stl            A-14b a closed cube and a disjoint open quad in ONE file
  a15_open_cube.stl, a15_lid.stl    A-15  a five-face cube and an independent sheet over the gap
  a9_lattice.stl, a9_sphere.stl     A-9   a strut lattice interpenetrating a sphere

The configs that use them (A-3r ranked, A-4b, A-9, A-12 non-cubic domain, A-16 the same cube
twice) are in run_acceptance.py's CASES. Coordinates follow the suite's off-lattice convention
(...17 / ...83) so no face lands on a lattice plane by accident. The output is deterministic, so
re-running leaves the committed files unchanged.
"""

import math
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))


def write_stl(name, tris):
    with open(os.path.join(HERE, name), "wb") as f:
        f.write(b"\0" * 80)
        f.write(struct.pack("<I", len(tris)))
        for a, b, c in tris:
            f.write(struct.pack("<3f", 0.0, 0.0, 0.0))
            for p in (a, b, c):
                f.write(struct.pack("<3f", *p))
            f.write(struct.pack("<H", 0))


def box(lo, hi, skip=()):
    x0, y0, z0 = lo
    x1, y1, z1 = hi
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
         (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    q = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    t = []
    for k, (a, b, c, d) in enumerate(q):
        if k in skip:
            continue
        t.append((v[a], v[b], v[c]))
        t.append((v[a], v[c], v[d]))
    return t


def icosphere(center, r, subdiv=3):
    p = (1 + 5 ** 0.5) / 2
    verts = [[-1, p, 0], [1, p, 0], [-1, -p, 0], [1, -p, 0], [0, -1, p], [0, 1, p],
             [0, -1, -p], [0, 1, -p], [p, 0, -1], [p, 0, 1], [-p, 0, -1], [-p, 0, 1]]
    faces = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4),
             (11, 10, 2), (10, 7, 6), (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8),
             (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1)]
    for _ in range(subdiv):
        mid, new = {}, []

        def m(i, j):
            k = (min(i, j), max(i, j))
            if k not in mid:
                a, b = verts[i], verts[j]
                verts.append([(a[t] + b[t]) / 2 for t in range(3)])
                mid[k] = len(verts) - 1
            return mid[k]

        for a, b, c in faces:
            ab, bc, ca = m(a, b), m(b, c), m(c, a)
            new += [(a, ab, ca), (b, bc, ab), (c, ca, bc), (ab, bc, ca)]
        faces = new
    out = []
    for a, b, c in faces:
        tri = []
        for i in (a, b, c):
            v = verts[i]
            n = math.sqrt(sum(t * t for t in v))
            tri.append(tuple(center[t] + r * v[t] / n for t in range(3)))
        out.append(tuple(tri))
    return out


def grid_sheet(f, lo, hi, n):
    """A sheet z = f(x, y) over [lo, hi]^2, n x n quads."""
    pts = [[(lo + (hi - lo) * i / n, lo + (hi - lo) * j / n) for j in range(n + 1)] for i in range(n + 1)]
    p = lambda i, j: (pts[i][j][0], pts[i][j][1], f(*pts[i][j]))
    t = []
    for i in range(n):
        for j in range(n):
            t.append((p(i, j), p(i + 1, j), p(i + 1, j + 1)))
            t.append((p(i, j), p(i + 1, j + 1), p(i, j + 1)))
    return t


def strut(p, q, w):
    lo = [min(p[i], q[i]) for i in range(3)]
    hi = [max(p[i], q[i]) for i in range(3)]
    for i in range(3):
        if hi[i] - lo[i] < 1e-9:
            lo[i] -= w / 2
            hi[i] += w / 2
    return box(tuple(lo), tuple(hi))


# A-5: sphere inside cube.
write_stl("a5_cube.stl", box((0.2317, 0.2317, 0.2317), (0.7683, 0.7683, 0.7683)))
write_stl("a5_sphere.stl", icosphere((0.5, 0.5, 0.5), 0.2, 3))

# A-11: an open sheet, tilted, crossing the box on all four sides (it extends past it).
write_stl("a11_sheet.stl", grid_sheet(lambda x, y: 0.4817 + 0.06 * x + 0.03 * y, -0.1, 1.1, 8))

# A-13: three defects on one sphere.
sphere = icosphere((0.5, 0.5, 0.5), 0.3, 3)
defective = list(sphere)
defective.append(sphere[17])                       # a duplicated triangle
a, b, c = defective[40]
defective[40] = (a, c, b)                          # a flipped one
a, b, c = defective[100]
defective[100] = ((a[0] + 1e-6, a[1], a[2]), b, c)  # its corner split 1e-6 from the neighbours'
write_stl("a13_sphere_defective.stl", defective)

# A-14a / A-14b: two shells in one file.
write_stl("a14a_two_cubes.stl", box((0.2017, 0.2017, 0.2017), (0.4583, 0.4583, 0.4583))
          + box((0.5417, 0.5417, 0.5417), (0.7983, 0.7983, 0.7983)))
quad = [((0.5417, 0.2017, 0.6017), (0.7983, 0.2017, 0.6017), (0.7983, 0.4583, 0.6017)),
        ((0.5417, 0.2017, 0.6017), (0.7983, 0.4583, 0.6017), (0.5417, 0.4583, 0.6017))]
write_stl("a14b_cube_and_quad.stl", box((0.2017, 0.2017, 0.2017), (0.4583, 0.4583, 0.4583)) + quad)

# A-15: a cube missing its top face (k = 1 in `box`), and the top face as its own sheet.
lo, hi = (0.2317, 0.2317, 0.2317), (0.7683, 0.7683, 0.7683)
write_stl("a15_open_cube.stl", box(lo, hi, skip=(1,)))
top = [t for k, t in enumerate(box(lo, hi)) if k in (2, 3)]
write_stl("a15_lid.stl", top)

# A-9: a coarse strut lattice interpenetrating a sphere.
lat = []
w = 0.04
for i in (0.2517, 0.5017, 0.7517):
    for j in (0.2517, 0.5017, 0.7517):
        lat += strut((0.1017, i, j), (0.9017, i, j), w)
        lat += strut((i, 0.1017, j), (i, 0.9017, j), w)
        lat += strut((i, j, 0.1017), (i, j, 0.9017), w)
write_stl("a9_lattice.stl", lat)
write_stl("a9_sphere.stl", icosphere((0.5017, 0.5017, 0.5017), 0.28, 3))
print("wrote A-5, A-9, A-11, A-13, A-14a/b, A-15 fixtures to", HERE)
