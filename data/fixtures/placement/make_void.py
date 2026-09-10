#!/usr/bin/env python3
"""Generate the void STL the placement example and tests run against.

Standard library only, and deliberately not the reader this repository uses: a
fixture written with the same code that reads it would prove nothing about
either.

The fixture is three non-overlapping icospheres. Spheres because a sphere's
distance to another sphere, and the volume of their intersection, both have
closed forms - so a test can check the placement engine against arithmetic
rather than against another of this crate's own functions. Three of them, apart,
so the void has several shells and the per-shell orientation check has something
real to run on.
"""

from __future__ import annotations

import argparse
import struct
import sys
import tempfile
from pathlib import Path

PHI = (1.0 + 5.0**0.5) / 2.0

BASE_VERTICES = [
    (-1.0, PHI, 0.0), (1.0, PHI, 0.0), (-1.0, -PHI, 0.0), (1.0, -PHI, 0.0),
    (0.0, -1.0, PHI), (0.0, 1.0, PHI), (0.0, -1.0, -PHI), (0.0, 1.0, -PHI),
    (PHI, 0.0, -1.0), (PHI, 0.0, 1.0), (-PHI, 0.0, -1.0), (-PHI, 0.0, 1.0),
]
BASE_FACES = [
    (0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11),
    (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6), (7, 1, 8),
    (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9),
    (4, 9, 5), (2, 4, 11), (6, 2, 10), (8, 6, 7), (9, 8, 1),
]

SPHERES = [
    ((30.0, 30.0, 30.0), 9.0),
    ((70.0, 35.0, 55.0), 7.0),
    ((45.0, 72.0, 40.0), 6.0),
]
LEVEL = 2


def icosphere(centre, radius, level):
    verts = [list(v) for v in BASE_VERTICES]
    faces = [list(f) for f in BASE_FACES]
    for _ in range(level):
        mid = {}
        out = []
        for a, b, c in faces:
            m = []
            for i, j in ((a, b), (b, c), (c, a)):
                key = (min(i, j), max(i, j))
                if key not in mid:
                    verts.append([(verts[i][k] + verts[j][k]) / 2.0 for k in range(3)])
                    mid[key] = len(verts) - 1
                m.append(mid[key])
            out += [[a, m[0], m[2]], [m[0], b, m[1]], [m[2], m[1], c], m]
        faces = out
    placed = []
    for v in verts:
        n = sum(x * x for x in v) ** 0.5
        placed.append(tuple(centre[k] + v[k] / n * radius for k in range(3)))
    return placed, faces


def build():
    triangles = []
    for centre, radius in SPHERES:
        verts, faces = icosphere(centre, radius, LEVEL)
        for a, b, c in faces:
            triangles.append((verts[a], verts[b], verts[c]))
    return triangles


def write_binary_stl(path, triangles):
    out = bytearray(b"placement void fixture".ljust(80, b"\0"))
    out += struct.pack("<I", len(triangles))
    for tri in triangles:
        out += struct.pack("<3f", 0.0, 0.0, 0.0)
        for vertex in tri:
            out += struct.pack("<3f", *vertex)
        out += struct.pack("<H", 0)
    data = bytes(out)
    path.write_bytes(data)
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?",
                        default=Path("data/input/placement/void_spheres.stl"))
    parser.add_argument("--check", action="store_true",
                        help="verify the file on disk matches what this script would write")
    args = parser.parse_args()

    triangles = build()
    expected_faces = len(SPHERES) * 20 * 4 ** LEVEL
    assert len(triangles) == expected_faces, (len(triangles), expected_faces)

    if args.check:
        if not args.output.is_file():
            print(f"{args.output} does not exist", file=sys.stderr)
            return 1
        with tempfile.TemporaryDirectory() as d:
            fresh = write_binary_stl(Path(d) / "v.stl", triangles)
        if fresh != args.output.read_bytes():
            print(f"{args.output} differs from what this script generates", file=sys.stderr)
            return 1
        print(f"{args.output}: {len(triangles)} triangles, matches")
        return 0

    args.output.parent.mkdir(parents=True, exist_ok=True)
    data = write_binary_stl(args.output, triangles)
    print(f"wrote {args.output}: {len(triangles)} triangles, {len(data)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
