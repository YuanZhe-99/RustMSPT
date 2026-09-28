#!/usr/bin/env python3
"""Measure this mesher against the shared reference dataset, at matched resolution.

P2 - "the fewest elements that satisfy P3 and P4" - is a comparison, and the comparison
had been quoted from memory in three separate sessions without a script behind it. This
is that script.

**Matched resolution, not matched settings.** Every case in the dataset carries the
reference run's own log, which states the domain it meshed, its base cell size, and how
many levels of local refinement it used. Those are parsed per case and turned into this
mesher's `h_max` / `h_min`, so both tools are asked to resolve the same features at the
same size and the element counts answer "how many elements does that cost", which is what
R2 gates. Comparing at each tool's default settings would compare the defaults.

The dataset lives outside this repository. Point `RUSTMSPT_REFERENCE_DATASET` at the
directory holding the case folders, or pass case directories as arguments. A case
directory is one containing `stl/` and the reference run's `*_mesh.log`.

Usage:
    python3 data/fixtures/meshgen/acceptance/run_reference.py [case_dir ...]
"""

import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
WORK = os.path.join(ROOT, "data", "output", "reference")
BIN = os.environ.get("RUSTMSPT_BIN", os.path.join(ROOT, "target", "release", "rustmspt"))
WORK = os.environ.get("RUSTMSPT_REFERENCE_WORK", WORK)

DEFAULT_ROOT = os.environ.get(
    "RUSTMSPT_REFERENCE_DATASET", "/home/yuanzhe/workspace/data"
)

CONFIG = """meshgen:
  inputs:
{inputs}
  domain:
    min: [{dmin}]
    max: [{dmax}]
  sizing:
    background: {{cells: [{counts}]}}
    max_level: {level}
    chord_error_frac: 0.2
    feature_angle_deg: 45.0
    grading: 2.0
    gap_cells: 4.0
    curve_cells: 2.0
  gaps:
    t_layer_factor: 1.0
    t_sheet_factor: 0.2
    confidence_min: 0.9
  envelope:
    eps_frac: 1.0e-4
  repair:
    level: conservative
  coincidence: merge
  fem_profile: implicit
  thin:
    enabled: true
  determinism: strict
  acceleration:
    mode: auto
  snapshots: key
  output:
    vtu: {out}
  verify:
    max_ar_warn: 20.0
    min_dihedral_deg: 5.0
"""

DOMAIN_RE = re.compile(
    r"Domain Boundaries:\s*\(([^)]*)\)\s*-\s*\(([^)]*)\)"
)
CELL_RE = re.compile(r"Base Mesh Element size:\s*([0-9.eE+-]+)")
LEVELS_RE = re.compile(r"SAMR levels:\s*(\d+)")


def discover(root):
    """Case directories under `root`: those carrying both `stl/` and a reference log."""
    out = []
    for entry in sorted(os.listdir(root)):
        case = os.path.join(root, entry)
        if os.path.isdir(os.path.join(case, "stl")) and glob.glob(
            os.path.join(case, "*_mesh.log")
        ):
            out.append(case)
    return out


def reference_inputs(case):
    """The reference tool's own INPUTS (plan M-1.8 (h)): `*RVE_CORNERS`, `*NUM_ELEMENT` and every
    section's `*SAMR_LEVEL`, from the case's `nurbs/*.nurbs`. The log's summary numbers are not the
    inputs: its `Base Mesh Element size` is the background cell's face diagonal (sqrt 2 x the
    edge), and its first `SAMR levels:` line is one section's level, not the run's (plan §2.3's
    correction, Appendix D.2)."""
    corners = counts = None
    levels = []
    by_stem = {}
    for f in sorted(glob.glob(os.path.join(case, "nurbs", "*.nurbs"))):
        lines = open(f, errors="replace").read().splitlines()
        for i, line in enumerate(lines):
            if line.startswith("*RVE_CORNERS") and corners is None:
                corners = [float(v) for v in lines[i + 1].split(",")]
            if line.startswith("*NUM_ELEMENT") and counts is None:
                counts = [int(v) for v in lines[i + 1].split(",")]
            if line.startswith("*SAMR_LEVEL"):
                levels.append(int(lines[i + 1].strip()))
                by_stem.setdefault(os.path.splitext(os.path.basename(f))[0], int(lines[i + 1].strip()))
    if corners is None or counts is None or not levels:
        return None
    return {"min": tuple(corners[:3]), "max": tuple(corners[3:6]), "counts": counts,
            "levels": levels, "by_stem": by_stem}


def reference_parameters(case):
    """Domain, base cell size and refinement depth, read from the reference run's log."""
    logs = sorted(glob.glob(os.path.join(case, "*_mesh.log")))
    if not logs:
        return None
    with open(logs[0], "r", errors="replace") as f:
        text = f.read(400000)
    domain = DOMAIN_RE.search(text)
    cell = CELL_RE.search(text)
    levels = LEVELS_RE.search(text)
    if not (domain and cell and levels):
        return None
    parse = lambda s: tuple(float(v) for v in s.split(","))
    return {
        "min": parse(domain.group(1)),
        "max": parse(domain.group(2)),
        "base_cell": float(cell.group(1)),
        "levels": int(levels.group(1)),
    }


def reference_cells(case):
    """Cell count of the reference tool's own result, from its VTU header."""
    for path in sorted(glob.glob(os.path.join(case, "*final.vtu"))):
        with open(path, "rb") as f:
            head = f.read(8192).decode("utf-8", "replace")
        m = re.search(r'NumberOfCells="(\d+)"', head)
        if m:
            return int(m.group(1))
    return None


def metrics_of(report, section):
    for s in report["sections"]:
        if s["id"] == section:
            return dict(s["metrics"]), s["status"]
    return {}, "ABSENT"


def run_case(case):
    name = os.path.basename(case.rstrip("/"))
    stls = sorted(glob.glob(os.path.join(case, "stl", "*.stl")))
    params = reference_parameters(case)
    if not stls or not params:
        return {"case": name, "error": "no STLs, or the reference log lacks its parameters"}

    # At the reference's own resolution (plan M-1.8 (h)): its background counts exactly, the
    # deepest section level globally, and each section's own level on its STL (particleN.stl is
    # section particleN.nurbs).
    ref = reference_inputs(case)
    if ref is None:
        return {"case": name, "error": "the reference inputs lack *RVE_CORNERS / *NUM_ELEMENT / *SAMR_LEVEL"}
    params["min"], params["max"] = ref["min"], ref["max"]
    span = [params["max"][i] - params["min"][i] for i in range(3)]
    diagonal = sum(s * s for s in span) ** 0.5
    levels = max(ref["levels"])
    h_max = max(span[i] / ref["counts"][i] for i in range(3))
    h_min = h_max / (2**levels)

    work = os.path.join(WORK, name)
    os.makedirs(work, exist_ok=True)
    config = os.path.join(work, "config.yaml")
    with open(config, "w") as f:
        f.write(
            CONFIG.format(
                inputs="\n".join(
                    f"    - stl: {s}"
                    + (
                        f"\n      max_level: {ref['by_stem'][os.path.splitext(os.path.basename(s))[0]]}"
                        if os.path.splitext(os.path.basename(s))[0] in ref["by_stem"]
                        else ""
                    )
                    for s in stls
                ),
                dmin=", ".join(f"{v}" for v in params["min"]),
                dmax=", ".join(f"{v}" for v in params["max"]),
                counts=", ".join(str(c) for c in ref["counts"]),
                level=levels,
                out=os.path.join(work, "mesh.vtu"),
            )
        )

    # P-2.1: the plain name is the delivered tets-only volume; the full catalog needs the
    # mixed-cell contract document beside it, because [V5]-[V9] read the face tags.
    vtu = os.path.join(work, "mesh.debug", "mesh_s08_cut_contract.vtu")
    js = os.path.join(work, "report.json")
    for stale in (vtu, js):
        if os.path.exists(stale):
            os.remove(stale)
    env = dict(os.environ, RUSTMSPT_CUT_DIAG="1")
    print(
        f"[{name}] {len(stls)} surfaces, domain diagonal {diagonal:.4f}, "
        f"h_max {h_max:.5g}, h_min {h_min:.5g} "
        f"(reference background {ref['counts']}, section levels {ref['levels']}; asked for level {levels})",
        file=sys.stderr,
    )
    sys.stderr.flush()
    mesh = subprocess.run(
        [BIN, "mesh", "--config", config], capture_output=True, text=True, env=env
    )
    if not os.path.exists(vtu):
        return {"case": name, "error": (mesh.stderr or mesh.stdout).strip()[-300:]}
    return lambda: verify_case(case, name, work, vtu, js, stls, env, params, levels, h_max, h_min)


def verify_case(case, name, work, vtu, js, stls, env, params, levels, h_max, h_min):
    vcfg = os.path.join(work, "verify.yaml")
    with open(vcfg, "w") as f:
        f.write(f"mesh_verify:\n  input: {vtu}\n  json: {js}\n  surfaces:\n")
        for stl in stls:
            f.write(f"    - {stl}\n")
        f.write("  verify:\n    max_ar_warn: 20.0\n    min_dihedral_deg: 5.0\n")
    verify = subprocess.run(
        [BIN, "mesh-verify", "--config", vcfg], capture_output=True, text=True, env=env
    )
    if not os.path.exists(js):
        return {"case": name, "error": verify.stderr.strip()[-300:]}
    with open(js) as f:
        report = json.load(f)
    v12, _ = metrics_of(report, "V12")
    v13, v13_status = metrics_of(report, "V13")
    ours = int(v12.get("tets", 0))
    theirs = reference_cells(case)
    return {
        "case": name,
        "surfaces": len(stls),
        "h_max": h_max,
        "h_min": h_min,
        "levels": params["levels"],
        "levels_used": levels,
        "tets": ours,
        "reference_cells": theirs,
        "ratio": ours / theirs if theirs else None,
        "cells": int(v12.get("lattice_cells", 0)),
        "tets_per_cell": v12.get("tets_per_lattice_cell", 0.0),
        "bg_tets": int(v12.get("tets_provenance_lattice", 0)),
        "cut_tets": int(v12.get("tets_provenance_cut", 0)),
        "cut_per_cell": v12.get("tets_per_cell_cut", 0.0),
        "fan_tets": int(v12.get("tets_provenance_junction", 0)),
        "fan_cells": int(v12.get("cells_provenance_junction", 0)),
        "fan_per_cell": v12.get("tets_per_cell_junction", 0.0),
        "V13": v13_status,
        "on_surface": v13.get("on_surface_area_frac", float("nan")),
        "dev_mean_h": v13.get("deviation_mean_frac_h", float("nan")),
        "offset_h": v13.get("offset_mean_frac_h", float("nan")),
        "disp_share": v13.get("displacement_share", float("nan")),
        "chord_mean_h": v13.get("chord_mean_frac_h", float("nan")),
    }


def main():
    cases = sys.argv[1:] or discover(DEFAULT_ROOT)
    if not cases:
        sys.exit(
            f"no case directories found under {DEFAULT_ROOT}\n"
            "set RUSTMSPT_REFERENCE_DATASET, or pass case directories as arguments"
        )
    os.makedirs(WORK, exist_ok=True)
    # Meshing stays serial (the mesher uses every core); the single-threaded `mesh-verify` of one
    # case overlaps the next case's meshing. Rows keep case order.
    from concurrent.futures import ThreadPoolExecutor

    with ThreadPoolExecutor(max_workers=3) as pool:
        futures = []
        for c in cases:
            pending = run_case(c)
            futures.append(pool.submit(pending) if callable(pending) else None)
            if not callable(pending):
                futures[-1] = pending
        rows = [f if isinstance(f, dict) else f.result() for f in futures]

    print()
    print("P2 - element count at matched resolution (R2)")
    print(
        "ratio < 1 means this mesher emits fewer elements. It is only a P2 result when P3 "
        "and P4 hold as well - fewer elements that do not lie on the surface is not a win."
    )
    header = (
        f"{'case':22} {'srf':>4} {'lvl':>5} {'ours':>10} {'reference':>10} {'ratio':>7} "
        f"{'cells':>10} {'t/cell':>7} {'fan%':>6} {'fan/cell':>9} {'cut/cell':>9}"
    )
    print(header)
    print("-" * len(header))
    for r in rows:
        if "error" in r:
            print(f"{r['case']:22} ERROR: {r['error']}")
            continue
        ratio = f"{r['ratio']:.2f}x" if r["ratio"] else "-"
        theirs = f"{r['reference_cells']:,}" if r["reference_cells"] else "-"
        fan = 100.0 * r["fan_tets"] / r["tets"] if r["tets"] else 0.0
        # "0>1" reads: the reference used no refinement, this mesher was given one level
        # because its config forbids h_min == h_max.
        level = (
            f"{r['levels']}>{r['levels_used']}"
            if r["levels"] != r["levels_used"]
            else str(r["levels"])
        )
        print(
            f"{r['case']:22} {r['surfaces']:>4} {level:>5} {r['tets']:>10,} "
            f"{theirs:>10} {ratio:>7} {r['cells']:>10,} {r['tets_per_cell']:>7.2f} "
            f"{fan:>6.1f} {r['fan_per_cell']:>9.2f} {r['cut_per_cell']:>9.2f}"
        )

    print()
    print("P3 - the same meshes, measured against their input surfaces ([V13])")
    print(
        "Read at the face CORNERS. chord/h is the sag of an anchored flat facet across "
        "curvature - not a P3 violation, and it is what refinement trades against P2."
    )
    header = (
        f"{'case':22} {'V13':>6} {'on%':>9} {'dev/h%':>8} {'off/h%':>8} {'disp':>6} "
        f"{'chord/h%':>9}"
    )
    print(header)
    print("-" * len(header))
    for r in rows:
        if "error" in r:
            continue
        print(
            f"{r['case']:22} {r['V13']:>6} {100.0 * r['on_surface']:>9.3f} "
            f"{100.0 * r['dev_mean_h']:>8.2f} {100.0 * r['offset_h']:>+8.2f} "
            f"{r['disp_share']:>6.3f} {100.0 * r['chord_mean_h']:>9.2f}"
        )

    out = os.path.join(WORK, "summary.json")
    with open(out, "w") as f:
        json.dump(rows, f, indent=2, sort_keys=True)
    print(f"\nwrote {out}")


if __name__ == "__main__":
    main()
