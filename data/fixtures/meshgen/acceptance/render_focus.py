#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10"]
# ///
"""Focus-region renders for one meshed case (PLAN_mesh_generation.md R9, M-1.9).

Why this exists: aggregates average away exactly the populations this project keeps finding.
The limb's serration was seen by eye before any metric could name it, and a pixel diff on a
fixed camera caught in one pass what three verifier metrics missed. So every change to what
the mesher emits is looked at - and looked at where the geometry is hard: where two bodies
meet (intersection curves), at sharp edges and corners.

The places come from the INPUT, never from the mesh. The s02 arranged snapshot lists every
curve the arrangement found (`CurveKind` 0 sharp, 1 rim, 2 intersection; 3 box is skipped);
the mesh declares only the curves it kept - on a8, 24 of 1,404 - so regions read off the output
would hide exactly what was lost.

Per sample point, three renders of the cut mesh (`s08` contract document), each framed on a
window a few minimum element sizes wide and centred on the point, with the input curve drawn as
red markers:

  surf+ / surf-   the material only (background removed), wireframe, coloured by region, seen
                  from the two sides of the curve. Does the boundary between two colours follow
                  the curve? Is a sharp edge carried by an element edge, or chamfered/serrated?
  cut             every cell whose centroid lies behind a plane through the point normal to the
                  curve, seen face on. The profile of the material across the edge: a corner
                  should be a corner.

Usage:
    uv run data/fixtures/meshgen/acceptance/render_focus.py CASE_WORK_DIR CASE [--before DIR]
                                                            [--per-kind N] [--width PX]

CASE_WORK_DIR is the directory `run_acceptance.py` wrote the case into (it holds
`<case>.yaml` and `<case>.debug/`). Output goes to `CASE_WORK_DIR/<case>.focus/`: one config
and one PNG per view, `manifest.json` (view -> point, tangent, curve, kind), and
`<case>_focus_sheet.png`. With `--before DIR` (an earlier `.focus/` directory) each view gets a
changed-pixel share in `diff.json` and a `diff_<view>.png` where the changes are red.
"""

import argparse
import json
import math
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
BIN = os.path.join(ROOT, "target", "release", "rustmspt")
KIND_NAME = {0: "sharp", 1: "rim", 2: "intersection"}


# ----------------------------------------------------------------- reading the s02 snapshot

def ascii_array(text, name):
    """The values of one ascii DataArray by name (the pipeline writes every snapshot in ascii,
    contracts D-19)."""
    m = re.search(r'<DataArray[^>]*Name="%s"[^>]*format="ascii"[^>]*>([^<]*)<' % re.escape(name), text)
    if m is None:
        return None
    return m.group(1).split()


def points_of(text):
    m = re.search(r"<Points>\s*<DataArray[^>]*>([^<]*)<", text)
    v = [float(x) for x in m.group(1).split()]
    return [tuple(v[i:i + 3]) for i in range(0, len(v), 3)]


def read_curves(s02_path):
    """Every non-box curve of the arrangement as a list of segments, keyed by curve id."""
    text = open(s02_path, encoding="latin1").read()
    pts = points_of(text)
    conn = [int(x) for x in ascii_array(text, "connectivity")]
    offs = [int(x) for x in ascii_array(text, "offsets")]
    types = [int(x) for x in ascii_array(text, "types")]
    curve_id = [int(x) for x in ascii_array(text, "curve_id")]
    kinds = [int(x) for x in (ascii_array(text, "CurveKind") or [])]
    comp_off = [int(x) for x in (ascii_array(text, "CurveCompOffsets") or [])]
    comp_mem = [int(x) for x in (ascii_array(text, "CurveCompComponents") or [])]

    def components(c):
        if c >= len(comp_off):
            return ()
        return tuple(sorted(comp_mem[(comp_off[c - 1] if c else 0):comp_off[c]]))

    curves = {}
    start = 0
    for i, end in enumerate(offs):
        if types[i] == 4 and curve_id[i] >= 0:
            c = curve_id[i]
            kind = kinds[c] if c < len(kinds) else -1
            if kind in KIND_NAME:
                nodes = conn[start:end]
                entry = curves.setdefault(c, {"kind": kind, "comps": components(c), "segs": []})
                for a, b in zip(nodes, nodes[1:]):
                    entry["segs"].append((pts[a], pts[b]))
        start = end
    return curves


# ----------------------------------------------------------------- choosing the places

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


def key(p):
    return tuple(round(x, 9) for x in p)


def along(segs, frac):
    """The point at a fraction of a curve's total length, and the tangent there."""
    total = sum(norm(sub(b, a)) for a, b in segs)
    target = frac * total
    for a, b in segs:
        l = norm(sub(b, a))
        if target <= l or (a, b) == segs[-1]:
            t = 0.0 if l == 0 else min(1.0, target / l)
            return add(a, scale(sub(b, a), t)), unit(sub(b, a))
        target -= l
    return segs[0][0], unit(sub(segs[0][1], segs[0][0]))


def choose_samples(curves, per_kind, spacing):
    """Deterministic sample points, spread over what differs:

    - for each (kind, component set) - a cube's edges, a limb's edges, the curve where the two
      meet - the longest `per_kind` curves at mid-length, so a short feature is not crowded out
      by a long one of another body;
    - up to `per_kind` corners, where three or more curve segments end, ranked by how many
      bodies meet there (a triple point where an intersection curve reaches a sharp edge first);
    - never two samples closer than `spacing`: two bodies in contact each declare the shared
      rim, so the same place arrives twice under two curve ids.
    """
    samples = []

    def far(p):
        return all(norm(sub(p, s["point"])) >= spacing for s in samples)

    groups = sorted({(e["kind"], e["comps"]) for e in curves.values()})
    for kind, comps in groups:
        ranked = sorted(
            (c for c, e in curves.items() if (e["kind"], e["comps"]) == (kind, comps)),
            key=lambda c: (-round(sum(norm(sub(b, a)) for a, b in curves[c]["segs"]), 12), c),
        )
        taken = 0
        for c in ranked:
            if taken == per_kind:
                break
            p, t = along(curves[c]["segs"], 0.5)
            if not far(p):
                continue
            samples.append({"name": f"{KIND_NAME[kind]}_c{c}", "kind": KIND_NAME[kind],
                            "curve": c, "point": p, "tangent": t,
                            "components": list(comps)})
            taken += 1
    ends = {}
    for c, e in curves.items():
        for a, b in e["segs"]:
            for p, q in ((a, b), (b, a)):
                ends.setdefault(key(p), []).append((c, unit(sub(q, p)), p))
    corners = []
    for k in sorted(ends):
        inc = ends[k]
        dirs = {(round(d[0], 6), round(d[1], 6), round(d[2], 6)) for _, d, _ in inc}
        if len(dirs) < 3:
            continue
        bodies = set()
        for c, _, _ in inc:
            bodies.update(curves[c]["comps"])
        has_x = any(curves[c]["kind"] == 2 for c, _, _ in inc)
        corners.append((-len(bodies), -int(has_x), k, inc, sorted(bodies)))
    corners.sort(key=lambda r: r[:3])
    taken = 0
    for _, _, _, inc, bodies in corners:
        if taken == per_kind:
            break
        p = inc[0][2]
        if not far(p):
            continue
        bisector = scale(add(add(inc[0][1], inc[1][1]), inc[2][1]), -1.0)
        t = unit(bisector) if norm(bisector) > 1e-9 else unit((1.0, 1.0, 1.0))
        samples.append({"name": "corner_" + "_".join("%.4f" % x for x in p).replace("-", "m"),
                        "kind": "corner", "curve": -1, "point": p, "tangent": t,
                        "components": bodies})
        taken += 1
    return samples


def finding_samples(report_path, per_code, spacing, existing):
    """Where the verifier says something failed: up to `per_code` located FAIL findings per code,
    so the picture and the metric are looked at in the same place (R9 item 4)."""
    if not os.path.exists(report_path):
        return []
    report = json.load(open(report_path))
    out = []
    for section in report["sections"]:
        by_code = {}
        for item in section["items"]:
            if item["severity"] == "FAIL" and item["coordinates"]:
                by_code.setdefault(item["code"], []).append(tuple(item["coordinates"][0]))
        for code in sorted(by_code):
            taken = 0
            for p in by_code[code]:
                if taken == per_code:
                    break
                if all(norm(sub(p, q["point"])) >= spacing for q in existing + out):
                    out.append({"name": f"{code.replace('.', '_')}_{taken}", "kind": "finding",
                                "curve": -1, "point": p, "tangent": unit((1.0, 1.0, 1.0)),
                                "components": [], "code": code})
                    taken += 1
    return out


# ----------------------------------------------------------------- rendering

def side_directions(t):
    """Two opposite view directions perpendicular to the tangent, fixed by the tangent alone."""
    ref = (0.0, 0.0, 1.0) if abs(t[2]) < 0.9 else (1.0, 0.0, 0.0)
    d = unit(cross(t, ref))
    d = unit(add(d, scale(unit(cross(t, d)), 0.5)))
    return d, scale(d, -1.0)


def curve_markers(curves, center, w, step):
    """Red markers along every input curve inside the window, one per `step` of length."""
    out = []
    for c in sorted(curves):
        for a, b in curves[c]["segs"]:
            l = norm(sub(b, a))
            n = max(1, int(l / step))
            for i in range(n + 1):
                p = add(a, scale(sub(b, a), i / n))
                if all(abs(p[j] - center[j]) <= w for j in range(3)):
                    out.append([round(x, 9) for x in p])
    uniq = sorted({tuple(p) for p in out})
    return [list(p) for p in uniq]


def yaml_list(v):
    return "[" + ", ".join(repr(float(x)) for x in v) + "]"


def write_view(cfg_dir, out_dir, name, mesh, center, w, view_dir, filters, markers, width):
    lo = [center[i] - w for i in range(3)]
    hi = [center[i] + w for i in range(3)]
    lines = [
        "mesh_render:",
        f"  input: {mesh}",
        f"  output_dir: {out_dir}",
        "  views:",
        f"    - name: {name}",
        f"      view_direction: {yaml_list(view_dir)}",
        f"      focus_point: {yaml_list(center)}",
        f"  width: {width}",
        f"  height: {width}",
        "  background: [255, 255, 255]",
        "  ambient: 0.35",
        "  color_by: region_key",
        "  backend: cpu",
        "  show_faces: false",
        "  show_curves: true",
        "  wireframe: true",
        "  projection: orthographic",
        "  fit_padding: 0.02",
        f"  frame_box: {{ min: {yaml_list(lo)}, max: {yaml_list(hi)} }}",
        "  filters:",
        # The filter keeps whole cells by centroid, so its border is a sawtooth of cell faces.
        # Filtering twice as wide as the frame puts that border outside the picture, where it
        # cannot be mistaken for serration of the mesh itself.
        f"    - {{ kind: bbox, min: {yaml_list([c - 2 * w for c in center])}, max: {yaml_list([c + 2 * w for c in center])} }}",
    ]
    lines += ["    - " + f for f in filters]
    if markers:
        lines.append("  highlight_points:")
        lines += [f"    - {yaml_list(p)}" for p in markers]
    path = os.path.join(cfg_dir, name + ".yaml")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    return path


def case_scale(case_yaml):
    """h_min from the case's own config: the window is a few of the finest elements wide."""
    text = open(case_yaml).read()
    frac = float(re.search(r"h_min_frac:\s*([0-9.eE+-]+)", text).group(1))
    lo = [float(x) for x in re.search(r"min:\s*\[([^\]]*)\]", text).group(1).split(",")]
    hi = [float(x) for x in re.search(r"max:\s*\[([^\]]*)\]", text).group(1).split(",")]
    diag = norm(sub(tuple(hi), tuple(lo)))
    return frac * diag


# ----------------------------------------------------------------- sheet and diff

def contact_sheet(out_dir, case, rows, width):
    from PIL import Image, ImageDraw
    tile = 512
    cols = 3
    label_h = 28
    sheet = Image.new("RGB", (cols * tile, len(rows) * (tile + label_h)), (255, 255, 255))
    draw = ImageDraw.Draw(sheet)
    for r, (sample, views) in enumerate(rows):
        for c, view in enumerate(views):
            png = os.path.join(out_dir, "png", f"{case}_s08_cut_contract_{view}.png")
            if not os.path.exists(png):
                continue
            img = Image.open(png).convert("RGB").resize((tile, tile))
            sheet.paste(img, (c * tile, r * (tile + label_h) + label_h))
            draw.text((c * tile + 6, r * (tile + label_h) + 6), view, fill=(0, 0, 0))
        draw.line([(0, r * (tile + label_h)), (cols * tile, r * (tile + label_h))], fill=(160, 160, 160))
    path = os.path.join(out_dir, f"{case}_focus_sheet.png")
    sheet.save(path)
    return path


def diff_against(out_dir, before_dir, case, views):
    from PIL import Image, ImageChops
    result = {}
    for view in views:
        name = f"{case}_s08_cut_contract_{view}.png"
        a, b = os.path.join(before_dir, "png", name), os.path.join(out_dir, "png", name)
        if not (os.path.exists(a) and os.path.exists(b)):
            result[view] = None
            continue
        ia, ib = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
        if ia.size != ib.size:
            result[view] = 1.0
            continue
        d = ImageChops.difference(ia, ib).convert("L").point(lambda x: 255 if x > 2 else 0)
        changed = sum(1 for x in d.getdata() if x) / (d.size[0] * d.size[1])
        result[view] = changed
        if changed > 0:
            red = Image.new("RGB", ib.size, (255, 0, 0))
            Image.composite(red, ib.point(lambda x: x // 2 + 127), d).save(
                os.path.join(out_dir, f"diff_{view}.png"))
    with open(os.path.join(out_dir, "diff.json"), "w") as f:
        json.dump(result, f, indent=1, sort_keys=True)
    return result


# ----------------------------------------------------------------- main

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("case")
    ap.add_argument("--before")
    ap.add_argument("--per-kind", type=int, default=4)
    ap.add_argument("--width", type=int, default=2048)
    ap.add_argument("--window", type=float, default=4.0, help="half-width in h_min")
    ap.add_argument("--findings", type=int, default=2, help="located FAIL findings per code")
    args = ap.parse_args()

    case, work = args.case, os.path.abspath(args.work)
    debug = os.path.join(work, case + ".debug")
    s02 = os.path.join(debug, f"{case}_s02_arranged.vtu")
    mesh = os.path.join(debug, f"{case}_s08_cut_contract.vtu")
    for p in (s02, mesh):
        if not os.path.exists(p):
            sys.exit(f"render_focus: {p} is missing (run the case with snapshots: key or all)")
    h_min = case_scale(os.path.join(work, case + ".yaml"))
    w = args.window * h_min

    curves = read_curves(s02)
    samples = choose_samples(curves, args.per_kind, w)
    samples += finding_samples(os.path.join(work, case + ".json"), args.findings, w, samples)
    out_dir = os.path.join(work, case + ".focus")
    cfg_dir = os.path.join(out_dir, "cfg")
    png_dir = os.path.join(out_dir, "png")
    os.makedirs(cfg_dir, exist_ok=True)
    os.makedirs(png_dir, exist_ok=True)

    rows, manifest = [], []
    for s in samples:
        p, t = s["point"], s["tangent"]
        markers = curve_markers(curves, p, w, h_min / 8.0)
        d1, d2 = side_directions(t)
        views = []
        for suffix, view_dir, filters in (
            ("surf+", d1, ["{ kind: background, keep: false }"]),
            ("surf-", d2, ["{ kind: background, keep: false }"]),
            ("cut", scale(t, -1.0), [f"{{ kind: clip_plane, origin: {yaml_list(p)}, normal: {yaml_list(t)} }}"]),
        ):
            name = f"{s['name']}_{suffix}"
            cfg = write_view(cfg_dir, png_dir, name, mesh, p, w, view_dir, filters, markers, args.width)
            r = subprocess.run([BIN, "mesh-render", "--config", cfg], capture_output=True, text=True)
            if r.returncode != 0:
                print(f"[focus] {name}: mesh-render failed\n{r.stdout}\n{r.stderr}", file=sys.stderr)
            views.append(name)
        rows.append((s, views))
        manifest.append({"sample": s["name"], "kind": s["kind"], "curve": s["curve"],
                         "components": s["components"],
                         "point": [round(x, 9) for x in p], "tangent": [round(x, 9) for x in t],
                         "window_half_width": w, "views": views})
    with open(os.path.join(out_dir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    sheet = contact_sheet(out_dir, case, rows, args.width)
    counts = {}
    for s in samples:
        counts[s["kind"]] = counts.get(s["kind"], 0) + 1
    print(f"[focus] {case}: {len(curves)} input curve(s); {len(samples)} sample(s) {counts}, "
          f"{3 * len(samples)} view(s), window +-{w:.4g}; sheet {sheet}")
    if args.before:
        diff = diff_against(out_dir, os.path.abspath(args.before), case,
                            [v for _, vs in rows for v in vs])
        moved = {k: v for k, v in diff.items() if v}
        print(f"[focus] {case}: {len(moved)} of {len(diff)} view(s) changed vs {args.before}")


if __name__ == "__main__":
    main()
