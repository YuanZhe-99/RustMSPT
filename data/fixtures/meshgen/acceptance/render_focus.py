#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10"]
# ///
"""Input against output, for one meshed case (PLAN_mesh_generation.md R9, M-1.9).

The question a picture has to answer is the owner's: what is the defect, where is it, and what
does it look like. So every picture here sets the INPUT surface (the STL the mesher was given)
beside the OUTPUT surface (the material boundary the mesh actually has - faces between elements
that disagree about which body they are in), from the same camera.

The output surface is coloured by how far each face's corners are from the input surface, as a
share of the face's own edge length - [V13]'s own measurement, exported by
`mesh-verify ... fidelity_vtu:`:

    grey    < 2 %       on the surface ([V13]'s tolerance for "on")
    yellow  2 - 10 %
    orange  10 - 25 %
    red     >= 25 %     a quarter of an element or more off the geometry
    blue    the input surface

Two kinds of picture:

  overview   the whole scene, input | output, from two opposite corners
  defects    faces 10 % or more off the surface, clustered by location; the clusters with the
             most off-surface area, each seen head-on and obliquely as input | output | both
             (the overlay is semi-transparent: where the output lies on the input they merge,
             where it does not you see both)

Usage:
    uv run data/fixtures/meshgen/acceptance/render_focus.py CASE_WORK_DIR CASE
           [--mesh PATH] [--defects N] [--width PX]

CASE_WORK_DIR holds `<case>.yaml`, `<case>_verify.yaml` and `<case>.debug/` as
`run_acceptance.py` writes them. Output: `CASE_WORK_DIR/<case>.focus/` with the comparison
surface, one PNG per panel, `summary.json`, and `<case>_compare_sheet.png`.
"""

import argparse
import json
import math
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
BIN = os.environ.get("RUSTMSPT_BIN", os.path.join(ROOT, "target", "release", "rustmspt"))


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def scale(a, s):
    return (a[0] * s, a[1] * s, a[2] * s)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def norm(a):
    return math.sqrt(dot(a, a))


def unit(a):
    n = norm(a)
    return scale(a, 1.0 / n) if n > 0 else a


def fmt(v):
    return "[" + ", ".join(repr(round(float(x), 9)) for x in v) + "]"


# ----------------------------------------------------------------- the comparison surface

def comparison_surface(work, case, mesh, out):
    """Run mesh-verify with `fidelity_vtu` on the case's own verify config."""
    base = open(os.path.join(work, case + "_verify.yaml")).read()
    base = re.sub(r"(?m)^  input: .*$", f"  input: {mesh}", base)
    base = re.sub(r"(?m)^  json: .*$", f"  json: {os.path.join(out, 'verify.json')}", base)
    path = os.path.join(out, case + ".compare.vtu")
    cfg = os.path.join(out, "verify.yaml")
    with open(cfg, "w") as f:
        f.write(base.rstrip("\n") + f"\n  fidelity_vtu: {path}\n")
    subprocess.run([BIN, "mesh-verify", "--config", cfg], capture_output=True, text=True)
    if not os.path.exists(path):
        sys.exit(f"render_focus: mesh-verify wrote no comparison surface (is `surfaces:` set in {cfg}?)")
    return path


def read_surface(path):
    text = open(path).read()

    def arr(name):
        m = re.search(r'<DataArray[^>]*Name="%s"[^>]*>([^<]*)<' % re.escape(name), text)
        return m.group(1).split()

    v = [float(x) for x in re.search(r"<Points>\s*<DataArray[^>]*>([^<]*)<", text).group(1).split()]
    pts = [tuple(v[i:i + 3]) for i in range(0, len(v), 3)]
    conn = [int(x) for x in arr("connectivity")]
    source = [int(x) for x in arr("source")]
    dev = [float(x) for x in arr("dev_pct")]
    comp = [int(x) for x in arr("component")]
    klass = [int(x) for x in arr("dev_class")]
    faces = []
    for i in range(len(source)):
        a, b, c = (pts[conn[3 * i + k]] for k in range(3))
        n = cross(sub(b, a), sub(c, a))
        faces.append({"source": source[i], "dev": dev[i], "component": comp[i],
                      "class": klass[i], "tri": (a, b, c),
                      "centroid": scale(add(add(a, b), c), 1.0 / 3.0),
                      "area": 0.5 * norm(n), "normal": unit(n)})
    return faces


def write_window(faces, lo, hi, path):
    """The faces whose centroid lies in [lo, hi], as a small surface VTU: a defect close-up then
    loads a few thousand triangles instead of the whole comparison surface."""
    keep = [f for f in faces if all(lo[i] <= f["centroid"][i] <= hi[i] for i in range(3))]
    pts = [p for f in keep for p in f["tri"]]
    n = len(keep)

    def arr(name, typ, vals, comps=""):
        return f'<DataArray type="{typ}" Name="{name}"{comps} format="ascii">' + " ".join(vals) + "</DataArray>\n"
    with open(path, "w") as o:
        o.write('<?xml version="1.0"?>\n<VTKFile type="UnstructuredGrid" version="1.0" '
                'byte_order="LittleEndian" header_type="UInt64">\n<UnstructuredGrid>\n')
        o.write(f'<Piece NumberOfPoints="{len(pts)}" NumberOfCells="{n}">\n<Points>\n')
        o.write(arr("Points", "Float64", (repr(c) for p in pts for c in p), ' NumberOfComponents="3"'))
        o.write("</Points>\n<Cells>\n")
        o.write(arr("connectivity", "Int64", (str(i) for i in range(len(pts)))))
        o.write(arr("offsets", "Int64", (str(3 * (i + 1)) for i in range(n))))
        o.write(arr("types", "UInt8", ("5" for _ in range(n))))
        o.write("</Cells>\n<CellData>\n")
        o.write(arr("source", "Int32", (str(f["source"]) for f in keep)))
        o.write(arr("dev_class", "Int32", (str(f["class"]) for f in keep)))
        o.write("</CellData>\n</Piece>\n</UnstructuredGrid>\n</VTKFile>\n")
    return path


# ----------------------------------------------------------------- choosing the defects

def clusters(faces, radius, threshold):
    """Greedy, deterministic clusters of faces at least `threshold` % off the surface, ranked by
    the area they carry."""
    bad = sorted((f for f in faces if f["source"] == 1 and f["dev"] >= threshold),
                 key=lambda f: (-f["dev"], f["centroid"]))
    out = []
    for f in bad:
        for c in out:
            if norm(sub(f["centroid"], c["seed"])) <= radius:
                c["faces"].append(f)
                break
        else:
            out.append({"seed": f["centroid"], "faces": [f]})
    for c in out:
        area = sum(f["area"] for f in c["faces"])
        c["area"] = area
        c["center"] = scale(
            (sum(f["centroid"][0] * f["area"] for f in c["faces"]),
             sum(f["centroid"][1] * f["area"] for f in c["faces"]),
             sum(f["centroid"][2] * f["area"] for f in c["faces"])), 1.0 / area)
        # Face normals carry the mesh's winding, which is not consistent; align them with the
        # first before averaging, so the camera looks at the surface rather than along it.
        ref = c["faces"][0]["normal"]
        acc = (0.0, 0.0, 0.0)
        for f in c["faces"]:
            n = f["normal"] if dot(f["normal"], ref) >= 0 else scale(f["normal"], -1.0)
            acc = add(acc, scale(n, f["area"]))
        c["normal"] = unit(acc) if norm(acc) > 0 else (0.0, 0.0, 1.0)
        c["max_dev"] = max(f["dev"] for f in c["faces"])
        c["components"] = sorted({f["component"] for f in c["faces"]})
    out.sort(key=lambda c: (-c["area"], c["seed"]))
    return out


# ----------------------------------------------------------------- rendering

def render(out, name, surface, view_dir, focus, frame, filters, colour, width, opacity=1.0, extra=()):
    """One mesh-render call; `extra` adds (name, view_dir) views that share its filters."""
    lines = ["mesh_render:", f"  input: {surface}", f"  output_dir: {os.path.join(out, 'png')}", "  views:"]
    for view_name, direction in [(name, view_dir)] + list(extra):
        lines += [f"    - name: {view_name}", f"      view_direction: {fmt(direction)}",
                  f"      focus_point: {fmt(focus)}"]
    lines += [
        f"  width: {width}",
        f"  height: {width}",
        "  background: [255, 255, 255]",
        "  ambient: 0.45",
        "  backend: cpu",
        "  wireframe: true",
        "  projection: orthographic",
        "  fit_padding: 0.0",
        f"  face_opacity: {opacity}",
        f"  frame_box: {{ min: {fmt(frame[0])}, max: {fmt(frame[1])} }}",
    ]
    lines += colour
    if filters:
        lines.append("  filters:")
        lines += ["    - " + f for f in filters]
    cfg = os.path.join(out, "cfg", name + ".yaml")
    with open(cfg, "w") as f:
        f.write("\n".join(lines) + "\n")
    r = subprocess.run([BIN, "mesh-render", "--config", cfg], capture_output=True, text=True)
    if r.returncode != 0:
        print(f"[focus] {name}: mesh-render failed\n{r.stdout[-400:]}\n{r.stderr[-400:]}", file=sys.stderr)
    stem = os.path.splitext(os.path.basename(surface))[0]
    return os.path.join(out, "png", f"{stem}_{name}.png")


INPUT = ["  color_by: uniform", "  uniform_color: [77, 121, 168]"]
OUTPUT = ["  color_by: dev_class"]
ONLY_INPUT = "{ kind: array_range, array: source, min: 0, max: 0 }"
ONLY_OUTPUT = "{ kind: array_range, array: source, min: 1, max: 1 }"


def box(center, w):
    return ([c - w for c in center], [c + w for c in center])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("case")
    ap.add_argument("--mesh", help="the mesh to compare (default: <case>.debug/<case>_s08_cut_contract.vtu)")
    ap.add_argument("--defects", type=int, default=6)
    ap.add_argument("--threshold", type=float, default=10.0, help="percent of an edge")
    ap.add_argument("--width", type=int, default=1200)
    args = ap.parse_args()

    work, case = os.path.abspath(args.work), args.case
    mesh = args.mesh or os.path.join(work, case + ".debug", f"{case}_s08_cut_contract.vtu")
    out = os.path.join(work, case + ".focus")
    os.makedirs(os.path.join(out, "cfg"), exist_ok=True)
    os.makedirs(os.path.join(out, "png"), exist_ok=True)
    surface = comparison_surface(work, case, mesh, out)
    faces = read_surface(surface)
    cfg = open(os.path.join(work, case + ".yaml")).read()
    h_min_frac = float(re.search(r"h_min_frac:\s*([0-9.eE+-]+)", cfg).group(1))
    lo = [float(x) for x in re.search(r"min:\s*\[([^\]]*)\]", cfg).group(1).split(",")]
    hi = [float(x) for x in re.search(r"max:\s*\[([^\]]*)\]", cfg).group(1).split(",")]
    h = h_min_frac * norm(sub(tuple(hi), tuple(lo)))

    # summary: output boundary area per class
    out_faces = [f for f in faces if f["source"] == 1]
    total = sum(f["area"] for f in out_faces) or 1.0
    bands = [(0.0, 2.0), (2.0, 10.0), (10.0, 25.0), (25.0, 1e30)]
    shares = [sum(f["area"] for f in out_faces if a <= f["dev"] < b) / total for a, b in bands]

    # scene extent from the input surface
    inp = [f["centroid"] for f in faces if f["source"] == 0]
    smin = [min(p[i] for p in inp) for i in range(3)]
    smax = [max(p[i] for p in inp) for i in range(3)]
    center = [(smin[i] + smax[i]) / 2 for i in range(3)]
    half = max(smax[i] - smin[i] for i in range(3)) * 0.62
    frame = box(center, half)

    panels = []
    stem = os.path.splitext(os.path.basename(surface))[0]
    png = lambda n: os.path.join(out, "png", f"{stem}_{n}.png")
    # one call per surface; the two corners are two views of it
    render(out, "overview_ne_input", surface, (-1.0, -1.0, -1.0), center, frame, [ONLY_INPUT], INPUT,
           args.width, extra=[("overview_sw_input", (1.0, 1.0, 1.0))])
    render(out, "overview_ne_output", surface, (-1.0, -1.0, -1.0), center, frame, [ONLY_OUTPUT], OUTPUT,
           args.width, extra=[("overview_sw_output", (1.0, 1.0, 1.0))])
    for tag in ("ne", "sw"):
        panels.append({"title": f"overview, from the {'+x+y+z' if tag == 'ne' else '-x-y-z'} corner",
                       "labels": ["input", "output"],
                       "files": [png(f"overview_{tag}_input"), png(f"overview_{tag}_output")]})

    w = 4.0 * h
    found = clusters(faces, 2.0 * w, args.threshold)
    defects = []
    for k, c in enumerate(found[:args.defects]):
        n = c["normal"]
        side = unit(cross(n, (0.0, 0.0, 1.0) if abs(n[2]) < 0.9 else (1.0, 0.0, 0.0)))
        oblique = unit(add(scale(n, -1.0), scale(side, 1.2)))
        fr = box(c["center"], w)
        window = write_window(faces, [x - 2 * w for x in c["center"]], [x + 2 * w for x in c["center"]],
                              os.path.join(out, f"window{k + 1}.vtu"))
        wstem = os.path.splitext(os.path.basename(window))[0]
        h_tag, o_tag = f"defect{k + 1}_head", f"defect{k + 1}_obl"
        head = scale(n, -1.0)
        render(out, h_tag + "_input", window, head, c["center"], fr, [ONLY_INPUT], INPUT, args.width,
               extra=[(o_tag + "_input", oblique)])
        render(out, h_tag + "_output", window, head, c["center"], fr, [ONLY_OUTPUT], OUTPUT, args.width,
               extra=[(o_tag + "_output", oblique)])
        render(out, h_tag + "_both", window, head, c["center"], fr, [], OUTPUT, args.width, opacity=0.55,
               extra=[(o_tag + "_both", oblique)])
        for view, tag in (("head-on", h_tag), ("oblique", o_tag)):
            panels.append({"title": f"defect {k + 1} ({view})", "labels": ["input", "output", "both"],
                           "files": [os.path.join(out, "png", f"{wstem}_{tag}_{s}.png")
                                     for s in ("input", "output", "both")]})
        defects.append({"rank": k + 1, "center": [round(x, 6) for x in c["center"]],
                        "faces": len(c["faces"]), "area": c["area"], "area_share": c["area"] / total,
                        "max_dev_pct": round(c["max_dev"], 1), "components": c["components"],
                        "window_half_width": w})

    summary = {"case": case, "mesh": mesh, "h_min": h, "boundary_faces": len(out_faces),
               "area_share_by_class": dict(zip(["<2%", "2-10%", "10-25%", ">=25%"], shares)),
               "clusters_at_or_above_threshold": len(found), "defects": defects, "panels": panels}
    with open(os.path.join(out, "summary.json"), "w") as f:
        json.dump(summary, f, indent=1)
    sheet(out, case, panels)
    print(f"[focus] {case}: {len(out_faces)} boundary faces; area on/2-10/10-25/>=25 % = "
          + " / ".join(f"{100 * s:.2f}" for s in shares)
          + f"; {len(found)} cluster(s) >= {args.threshold:g} %, {len(defects)} shown")


def sheet(out, case, panels):
    from PIL import Image, ImageDraw
    tile, label = 420, 26
    cols = 3
    im = Image.new("RGB", (cols * tile, len(panels) * (tile + label)), (255, 255, 255))
    draw = ImageDraw.Draw(im)
    for r, p in enumerate(panels):
        y = r * (tile + label)
        draw.text((6, y + 6), p["title"], fill=(0, 0, 0))
        for c, (lab, f) in enumerate(zip(p["labels"], p["files"])):
            if os.path.exists(f):
                im.paste(Image.open(f).convert("RGB").resize((tile, tile)), (c * tile, y + label))
            draw.text((c * tile + 6, y + label + 4), lab, fill=(0, 0, 0))
    im.save(os.path.join(out, f"{case}_compare_sheet.png"))


if __name__ == "__main__":
    main()
