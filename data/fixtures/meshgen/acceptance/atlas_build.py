#!/usr/bin/env python3
"""Build the data behind the mesh Atlas (the 3D case browser): one binary per case plus a manifest.

    python3 data/fixtures/meshgen/acceptance/atlas_build.py OUT WORK_DIR [WORK_DIR ...]

Each WORK_DIR is a `run_acceptance.py` path directory (`.../acceptance_<tag>/gated`) or a
`run_reference.py` case directory's parent (`.../ref`). For every case found there it reads the
exported contract document (`<case>_contract.vtu`, or `mesh_contract.vtu` for a reference case),
its verifier report and its input STLs, and writes `OUT/data/<case>.bin` and one
`OUT/manifest.json`. A case found in two directories takes the later one, so newer runs override.

Per case, from the mesh alone:
  - the material boundary: every tet face whose two tets carry different region keys, plus every
    sheet face (FaceTagKind = 1), with the two region keys;
  - a cross-section of the volume mesh through the middle of the box on each axis: the polygon each
    cut tet leaves, filled by its region and outlined, so the elements themselves can be seen;
  - the input surfaces, for an overlay.
Positions are float32, indices uint32, region keys uint16; the manifest gives every block's byte
offset and length. Needs numpy.
"""

import json
import os
import re
import struct
import sys

import numpy as np

ARRAY = re.compile(rb'<DataArray([^>]*)>([^<]*)</DataArray>', re.S)
NAME = re.compile(rb'Name="([^"]+)"')


def read_vtu(path):
    data = open(path, "rb").read()
    arrays = {}
    points = None
    p_at = data.find(b"<Points>")
    for m in ARRAY.finditer(data):
        attrs, body = m.group(1), m.group(2)
        name = NAME.search(attrs)
        floaty = b"Float" in attrs
        dtype = np.float64 if floaty else (np.uint64 if b'"UInt64"' in attrs else np.int64)
        values = np.array(body.split(), dtype=dtype)
        if p_at >= 0 and m.start() > p_at and points is None and name and name.group(1) == b"Points":
            points = values.reshape(-1, 3)
            continue
        if name:
            arrays[name.group(1).decode()] = values
    if points is None:
        points = arrays.pop("Points").reshape(-1, 3)
    return points, arrays


def csr(offsets, members):
    out, start = [], 0
    for end in offsets:
        out.append([int(x) for x in members[start:int(end)]])
        start = int(end)
    return out


def read_stl(path):
    raw = open(path, "rb").read()
    n = struct.unpack("<I", raw[80:84])[0]
    if 84 + 50 * n == len(raw):
        rec = np.frombuffer(raw[84:84 + 50 * n], dtype=np.dtype([("n", "<f4", 3), ("v", "<f4", 9), ("a", "<u2")]))
        return rec["v"].reshape(-1, 3, 3).astype(np.float32)
    tris, cur = [], []
    for line in raw.decode(errors="ignore").splitlines():
        line = line.strip()
        if line.startswith("vertex"):
            cur.append([float(x) for x in line.split()[1:4]])
            if len(cur) == 3:
                tris.append(cur)
                cur = []
    return np.array(tris, dtype=np.float32).reshape(-1, 3, 3)


FACES = np.array([[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]])
EDGES = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]


def material_boundary(points, tets, rk, sheet_faces):
    t = tets.shape[0]
    f = np.sort(tets[:, FACES].reshape(-1, 3), axis=1)
    owner = np.repeat(np.arange(t), 4)
    order = np.lexsort((f[:, 2], f[:, 1], f[:, 0]))
    fs, os_ = f[order], owner[order]
    same = np.all(fs[1:] == fs[:-1], axis=1)
    pairs = np.nonzero(same)[0]
    a, b = os_[pairs], os_[pairs + 1]
    diff = rk[a] != rk[b]
    faces = fs[pairs][diff]
    keys = np.stack([rk[a][diff], rk[b][diff]], axis=1)
    if len(sheet_faces):
        sf = np.sort(np.array(sheet_faces), axis=1)
        faces = np.concatenate([faces, sf])
        keys = np.concatenate([keys, np.full((len(sf), 2), -1)])
    return faces, keys


def section(points, tets, rk, axis, value):
    p = points[tets]  # (T, 4, 3)
    d = p[:, :, axis] - value
    cut = (d.min(axis=1) < 0) & (d.max(axis=1) > 0)
    p, d, regions = p[cut], d[cut], rk[cut]
    polys, outlines, keys = [], [], []
    others = [k for k in range(3) if k != axis]
    for tet, dd, key in zip(p, d, regions):
        pts = []
        for i, j in EDGES:
            if (dd[i] < 0) != (dd[j] < 0):
                s = dd[i] / (dd[i] - dd[j])
                pts.append(tet[i] + s * (tet[j] - tet[i]))
        if len(pts) < 3:
            continue
        pts = np.array(pts)
        c = pts.mean(axis=0)
        ang = np.arctan2(pts[:, others[1]] - c[others[1]], pts[:, others[0]] - c[others[0]])
        pts = pts[np.argsort(ang)]
        for k in range(1, len(pts) - 1):
            polys.append([pts[0], pts[k], pts[k + 1]])
            keys.append(key)
        for k in range(len(pts)):
            outlines.append([pts[k], pts[(k + 1) % len(pts)]])
    return (np.array(polys, dtype=np.float32).reshape(-1, 3, 3), np.array(keys, dtype=np.uint16),
            np.array(outlines, dtype=np.float32).reshape(-1, 2, 3))


def statuses(report):
    out = {}
    for s in report.get("sections", []):
        m = dict(s["metrics"])
        out[s["id"]] = {"status": s["status"], **{k: m[k] for k in m if k in (
            "on_surface_area_frac", "contained_area_frac", "covered_area_frac", "tets", "negative_volume",
            "duplicate_nodes", "hanging_nodes", "folded_faces", "boundary_leaks", "non_manifold_edges",
            "misattributed_cells", "region_adjacency_violations", "curve_nodes_missing_id", "partitions",
            "sheet_faces", "min_dihedral_deg", "max_aspect_ratio", "inp_elements")
            or k.endswith("_volume_error")}}
    return out


def find_cases(work):
    """(case, contract, report, verify yaml) found in one work directory."""
    found = []
    if os.path.basename(work.rstrip("/")) == "ref":
        for case in sorted(os.listdir(work)):
            d = os.path.join(work, case)
            vtu = os.path.join(d, "mesh_contract.vtu")
            if os.path.exists(vtu):
                found.append((case, vtu, os.path.join(d, "report.json"), os.path.join(d, "verify.yaml")))
        return found
    for f in sorted(os.listdir(work)):
        if f.endswith("_contract.vtu") and not f.startswith("mesh"):
            case = f[: -len("_contract.vtu")]
            found.append((case, os.path.join(work, f), os.path.join(work, case + ".json"),
                          os.path.join(work, case + "_verify.yaml")))
    return found


def stls_of(verify_yaml):
    out = []
    if not os.path.exists(verify_yaml):
        return out
    for line in open(verify_yaml):
        m = re.match(r"\s*-\s*(?:\{stl:\s*)?([^,}]+\.stl)", line)
        if m:
            out.append(m.group(1).strip())
    return out


def build_case(case, vtu, report_path, verify_yaml, out_dir):
    points, a = read_vtu(vtu)
    types, offsets, conn = a["types"], a["offsets"], a["connectivity"]
    tet_cells = np.nonzero(types == 10)[0]
    starts = np.concatenate([[0], offsets[:-1]])
    tets = np.stack([conn[starts[tet_cells] + k] for k in range(4)], axis=1)
    rk = a["region_key"][tet_cells].astype(np.int64)
    sets = csr(a.get("RegionSetOffsets", []), a.get("RegionSetComponents", []))
    sheet_faces = []
    if "face_tag_key" in a and "FaceTagKind" in a:
        kinds = a["FaceTagKind"]
        for c in np.nonzero(types == 5)[0]:
            k = a["face_tag_key"][c]
            if k >= 0 and kinds[k] == 1:
                sheet_faces.append(conn[starts[c]:starts[c] + 3])
    faces, keys = material_boundary(points, tets, rk, sheet_faces)
    used, inv = np.unique(faces.ravel(), return_inverse=True)
    verts = points[used].astype(np.float32)
    idx = inv.reshape(-1, 3).astype(np.uint32)
    lo, hi = points.min(axis=0), points.max(axis=0)
    blocks = [("verts", verts.tobytes()), ("faces", idx.tobytes()),
              ("face_keys", np.where(keys < 0, 65535, keys).astype(np.uint16).tobytes())]
    sections = []
    for axis in range(3):
        value = lo[axis] + 0.5137 * (hi[axis] - lo[axis])  # off every lattice plane
        polys, pkeys, lines = section(points, tets, rk, axis, value)
        blocks += [(f"slice{axis}_tris", polys.tobytes()), (f"slice{axis}_keys", pkeys.tobytes()),
                   (f"slice{axis}_lines", lines.tobytes())]
        sections.append({"axis": axis, "value": float(value), "polygons": int(len(pkeys))})
    inputs = []
    for k, stl in enumerate(stls_of(verify_yaml)):
        if os.path.exists(stl):
            tris = read_stl(stl)
            blocks.append((f"input{k}", tris.tobytes()))
            inputs.append({"name": os.path.basename(stl), "triangles": int(len(tris))})
    layout, buf, at = {}, bytearray(), 0
    for name, data in blocks:
        pad = (-at) % 4
        buf += b"\0" * pad
        at += pad
        layout[name] = [at, len(data)]
        buf += data
        at += len(data)
    # Published files are capped at 16 MB, so a large case is split into parts the viewer joins.
    parts = []
    chunk = 15 * 1024 * 1024
    for k in range(0, max(len(buf), 1), chunk):
        name = f"data/{case}.{k // chunk}.wasm"  # served as bytes; the page only fetches them
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(bytes(buf[k:k + chunk]))
        parts.append(name)
    report = json.load(open(report_path)) if os.path.exists(report_path) else {}
    return {
        "case": case,
        "bin": parts,
        "bytes": len(buf),
        "layout": layout,
        "box": [lo.tolist(), hi.tolist()],
        "tets": int(len(tets)),
        "boundary_faces": int(len(idx)),
        "sheet_faces": int(len(sheet_faces)),
        "regions": [{"key": k, "components": s} for k, s in enumerate(sets)],
        "sections": sections,
        "inputs": inputs,
        "checks": statuses(report),
    }


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(os.path.join(out, "data"), exist_ok=True)
    chosen = {}
    for work in sys.argv[2:]:
        for case, vtu, rep, vy in find_cases(work):
            chosen[case] = (vtu, rep, vy, work)
    manifest = []
    for case in sorted(chosen):
        vtu, rep, vy, work = chosen[case]
        entry = build_case(case, vtu, rep, vy, out)
        entry["source"] = work
        manifest.append(entry)
        print(f"{case}: {entry['tets']} tets, {entry['boundary_faces']} boundary faces, {entry['bytes'] / 1e6:.1f} MB", flush=True)
    json.dump({"cases": manifest}, open(os.path.join(out, "manifest.json"), "w"))


if __name__ == "__main__":
    main()
