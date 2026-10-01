#!/usr/bin/env python3
"""Rotated copies of the thin-feature and lattice acceptance cases (a6a, a6b, a7a, a7b, a8).

Every original fixture is axis-aligned, so its faces, edges and contact planes lie on or
parallel to lattice planes - which is exactly the configuration the cut was tuned on. These
copies turn each case by 23 degrees about the axis (1, 2, 3) through the domain centre, so
no face, edge or contact plane is parallel to any lattice direction. Nothing is scaled:
the a6/a7 thicknesses are sized against the converged h and must survive unchanged.

    python3 data/fixtures/meshgen/acceptance/generate_rotated_cases.py
"""
import math
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
ANGLE_DEG = 23.0
AXIS = (1.0, 2.0, 3.0)
CENTRE = (0.5, 0.5, 0.5)
SOURCES = [
    "a6a_cube.stl", "a6a_limb.stl",
    "a6b_cube.stl", "a6b_limb.stl",
    "a7a_lower.stl", "a7a_upper.stl",
    "a7b_lower.stl", "a7b_upper.stl",
    "a8_lattice.stl",
]


def read_stl(path):
    data = open(path, "rb").read()
    n = struct.unpack("<I", data[80:84])[0]
    tris = []
    for i in range(n):
        base = 84 + 50 * i
        v = struct.unpack("<12f", data[base:base + 48])
        tris.append((v[3:6], v[6:9], v[9:12]))
    return tris


def write_stl(path, tris):
    with open(path, "wb") as f:
        f.write(b"rotated acceptance fixture".ljust(80, b" "))
        f.write(struct.pack("<I", len(tris)))
        for a, b, c in tris:
            u = [b[k] - a[k] for k in range(3)]
            w = [c[k] - a[k] for k in range(3)]
            n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
            length = math.sqrt(sum(x * x for x in n)) or 1.0
            f.write(struct.pack("<3f", *[x / length for x in n]))
            for p in (a, b, c):
                f.write(struct.pack("<3f", *p))
            f.write(b"\0\0")


def rotation():
    norm = math.sqrt(sum(x * x for x in AXIS))
    x, y, z = (c / norm for c in AXIS)
    t = math.radians(ANGLE_DEG)
    c, s, C = math.cos(t), math.sin(t), 1.0 - math.cos(t)
    return [
        [c + x * x * C, x * y * C - z * s, x * z * C + y * s],
        [y * x * C + z * s, c + y * y * C, y * z * C - x * s],
        [z * x * C - y * s, z * y * C + x * s, c + z * z * C],
    ]


def main():
    r = rotation()

    def turn(p):
        q = [p[k] - CENTRE[k] for k in range(3)]
        return tuple(sum(r[i][j] * q[j] for j in range(3)) + CENTRE[i] for i in range(3))

    for name in SOURCES:
        tris = [tuple(turn(p) for p in t) for t in read_stl(os.path.join(HERE, name))]
        lo = min(min(p[k] for t in tris for p in t) for k in range(3))
        hi = max(max(p[k] for t in tris for p in t) for k in range(3))
        if lo <= 0.0 or hi >= 1.0:
            raise SystemExit(f"{name} leaves the unit domain after rotation ({lo:.4f}, {hi:.4f})")
        out = name.replace(".stl", "_r.stl")
        write_stl(os.path.join(HERE, out), tris)
        print(f"{out}: {len(tris)} triangles, extent [{lo:.4f}, {hi:.4f}]")


if __name__ == "__main__":
    main()
