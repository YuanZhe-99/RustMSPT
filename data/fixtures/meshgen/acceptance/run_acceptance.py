#!/usr/bin/env python3
"""Run the nine acceptance cases end to end and print the goal's metric tables.

Why this exists: the cases' configs lived only in shell history. Every session that
measured them rebuilt the configs by hand, so a number quoted in one session could not
be reproduced in the next - and twice a conclusion was argued from a measurement that
no longer existed anywhere. The configs are the experiment; they belong in the repo.

Three tables, one per goal property that has a number (PLAN Part I §1):

  P3  exact surfaces  - [V13], the material boundary measured against the input surface.
                        `on%` is the share of boundary area that lies *on* it; P3 is met
                        only at 100. `disp` separates a boundary that is displaced from
                        one that is merely rough - see [V13]'s note.
  P2  minimum elements - [V12]/[V13], the element count and where the elements came from,
                        normalised by input surface area so two meshers on the same STL
                        are comparable.
  P-2.1 the deliverable - is the file we hand over a tets-only mesh whose only feature
                        edges are the domain box? Read off the delivered file alone.
  P1/P4               - the check statuses, unchanged.

Usage:
    python3 data/fixtures/meshgen/acceptance/run_acceptance.py [case ...]
    python3 data/fixtures/meshgen/acceptance/run_acceptance.py --json baseline.json

With no arguments it runs all nine. `RUSTMSPT_CUT_DIAG` is set for every run so the
`parent_cell` array is present - it is what turns element counts into per-lattice-cell
emission rates, and it lets the undeclared-boundary metric separate a defect *inside*
one escalated cell from a disagreement between two.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
HERE = os.path.dirname(os.path.abspath(__file__))
WORK = os.path.join(ROOT, "data", "output", "acceptance")
BIN = os.path.join(ROOT, "target", "release", "rustmspt")

# (case, input STLs, sizing overrides). A-6's h_max_frac 0.04 is not a taste: the limb
# thicknesses are sized against the *converged* h, and 0.04 converges to h = 0.034641,
# which is what puts 0.0060 in the sheet regime and 0.0200 in the band regime. See the
# note in generate_acceptance_cases.py.
# The matrix runs h_max_frac 0.05 / h_min_frac 0.012 (PLAN_mesh_generation.md §17.4,
# "S0-S8 and mesh-verify at h_max_frac = 0.05, h_min_frac = 0.012"), with two recorded
# per-case exceptions:
#   A-4  h_max_frac 0.0125 - at the matrix-wide 0.05 the cube is 2.7 cells across and
#        reads 25.5 % short; the sweep 0.05 -> 0.025 -> 0.0125 gives 25.50 -> 5.79 ->
#        0.82 %, so this is resolution and the fixture owns its own cap (PLAN §17.4).
#   A-6  h_max_frac 0.04 - the limb thicknesses are sized against the *converged* h and
#        0.04 converges to h = 0.034641, which is what puts 0.0060 in the sheet regime
#        and 0.0200 in the band regime (generate_acceptance_cases.py) - plus the config
#        default floor 0.002, because the matrix floor 0.012 is twice the 0.0060 limb and
#        nothing refines toward it (measured: 701 undeclared faces against 2,216).
CASES = [
    ("a1", ["a1_sphere.stl"], {}),
    ("a2", ["a2_cube.stl"], {}),
    ("a3", ["a3_cube.stl", "a3_sphere.stl"], {}),
    ("a4", ["a4_cube.stl", "a4_sphere.stl"], {"h_max_frac": 0.0125}),
    ("a6a", ["a6a_cube.stl", "a6a_limb.stl"], {"h_max_frac": 0.04, "h_min_frac": 0.004}),
    ("a6b", ["a6b_cube.stl", "a6b_limb.stl"], {"h_max_frac": 0.04, "h_min_frac": 0.004}),
    ("a7a", ["a7a_lower.stl", "a7a_upper.stl"], {}),
    ("a7b", ["a7b_lower.stl", "a7b_upper.stl"], {}),
    ("a8", ["a8_lattice.stl"], {}),
]

CONFIG = """meshgen:
  inputs:
{inputs}
  domain:
    min: [0.0, 0.0, 0.0]
    max: [1.0, 1.0, 1.0]
  sizing:
    h_max_frac: {h_max_frac}
    h_min_frac: {h_min_frac}
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
    # Every snapshot writes the delivered tets-only volume under the plain name and the
    # mixed-cell contract document beside it as `_contract.vtu` (P-2.1). No setting.
    vtu: {out}
  verify:
    max_ar_warn: 20.0
    min_dihedral_deg: 5.0
"""


def write_config(case, stls, overrides):
    inputs = "\n".join(
        "    - stl: {}".format(os.path.join(HERE, s)) for s in stls
    )
    text = CONFIG.format(
        inputs=inputs,
        h_max_frac=overrides.get("h_max_frac", 0.05),
        h_min_frac=overrides.get("h_min_frac", 0.012),
        out=os.path.join(WORK, case + ".vtu"),
    )
    path = os.path.join(WORK, case + ".yaml")
    with open(path, "w") as f:
        f.write(text)
    return path


def metrics_of(report, section):
    for s in report["sections"]:
        if s["id"] == section:
            return dict(s["metrics"]), s["status"]
    return {}, "ABSENT"


def check_delivered(path, env):
    """P-2.1's acceptance, per case: is the file we hand over actually the mesh?

    Three claims, all read off the delivered file itself and none of them from the
    contract document beside it:

      tets_only   nothing in the file but VTK_TETRA (10), so no filter is needed to see it
      box_only    [V3] fires nothing - every single-owner face is on a domain plane, none
                  is shared by more than two tets, no edge is non-manifold. On a document
                  carrying no tags that IS "the only feature edges are the domain box"
      region      region identity rides on a cell array, not on separate triangle cells

    All three come from `mesh-verify` run on the delivered file, not from parsing it here.
    The first version scanned the XML for the `types` array and reported eight of nine cases
    as "not tets-only" - because it read the first 2 MB and the `types` block sits past that
    on every mesh bigger than a1. The tool already knows: [V12] counts cells and tets, and
    [V6] skips with a named reason when `region_key` is absent.
    """
    out = {"exists": os.path.exists(path), "tets_only": False, "box_only": False,
           "region": False, "cells": 0, "tets": 0, "codes": []}
    if not out["exists"]:
        return out
    js = os.path.splitext(path)[0] + "_delivered.json"
    cfg = os.path.splitext(path)[0] + "_delivered.yaml"
    with open(cfg, "w") as f:
        f.write(f"mesh_verify:\n  input: {path}\n  json: {js}\n")
    subprocess.run([BIN, "mesh-verify", "--config", cfg], capture_output=True, text=True, env=env)
    if not os.path.exists(js):
        return out
    with open(js) as f:
        report = json.load(f)
    v12, _ = metrics_of(report, "V12")
    out["cells"] = int(v12.get("cells", 0))
    out["tets"] = int(v12.get("tets", 0))
    out["tets_only"] = out["tets"] > 0 and out["cells"] == out["tets"]
    # [V6] needs `region_key`; it reports SKIPPED naming the missing array when it is not
    # there, so a section that ran is proof the identity is carried on the cells.
    _, v6_status = metrics_of(report, "V6")
    out["region"] = v6_status not in ("SKIPPED", "ABSENT")
    codes = [
        i["code"]
        for s in report["sections"]
        if s["id"] in ("V1", "V3")
        for i in s["items"]
        if i["severity"] in ("WARN", "FAIL")
    ]
    out["codes"] = sorted(set(codes))
    out["box_only"] = not out["codes"]
    return out


def status_of(report, section):
    return metrics_of(report, section)[1]


def run(case, stls, overrides):
    config = write_config(case, stls, overrides)
    # P-2.1: `<case>_s08_cut.vtu` is the DELIVERED mesh - tets only, region identity on a
    # cell array - and `<case>_s08_cut_contract.vtu` beside it is the mixed-cell document
    # carrying the tags. The full catalog is read off the contract file, because [V5]-[V9]
    # need those tags; the delivered file gets its own structural pass below, which is the
    # acceptance criterion "opened directly, the only feature edges are the domain box".
    delivered = os.path.join(WORK, case + ".debug", case + "_s08_cut.vtu")
    contract = os.path.join(WORK, case + ".debug", case + "_s08_cut_contract.vtu")
    # Clear last run's artefacts first. Without this a run that fails validation leaves
    # the previous mesh on disk, the existence check below passes, and the table reports
    # the *old* mesh under the new settings - two different configs silently produced
    # byte-identical rows before this was added.
    for stale in (
        delivered,
        contract,
        os.path.join(WORK, case + ".json"),
        os.path.join(WORK, case + "_delivered.json"),
        # Left behind by runs from before P-2.1, when the tets-only file was an optional
        # `_volume` companion. Nothing writes it now, so an old one lying beside the current
        # artefacts is exactly the "reported the previous mesh under the new settings" trap
        # this list already exists to close.
        os.path.join(WORK, case + ".debug", case + "_s08_cut_volume.vtu"),
        os.path.join(WORK, case + ".debug", case + "_s05_lattice_volume.vtu"),
    ):
        if os.path.exists(stale):
            os.remove(stale)
    env = dict(os.environ, RUSTMSPT_CUT_DIAG="1")
    mesh = subprocess.run(
        [BIN, "mesh", "--config", config], capture_output=True, text=True, env=env
    )
    # `mesh` exits non-zero by design - S9..S11 are unimplemented - after writing the
    # s08 cut snapshot, which is the mesh these gates are measured on.
    if not os.path.exists(contract):
        return {"case": case, "error": [(mesh.stderr or mesh.stdout).strip()[-200:]]}
    js = os.path.join(WORK, case + ".json")
    vcfg = os.path.join(WORK, case + "_verify.yaml")
    with open(vcfg, "w") as f:
        f.write("mesh_verify:\n  input: {}\n  json: {}\n  surfaces:\n".format(contract, js))
        for stl in stls:
            f.write("    - {}\n".format(os.path.join(HERE, stl)))
        f.write("  verify:\n    max_ar_warn: 20.0\n    min_dihedral_deg: 5.0\n")
    verify = subprocess.run(
        [BIN, "mesh-verify", "--config", vcfg],
        capture_output=True,
        text=True,
        env=env,
    )
    if not os.path.exists(js):
        return {"case": case, "error": [verify.stderr.strip()[-200:]]}
    with open(js) as f:
        report = json.load(f)
    deliverable = check_delivered(delivered, env)
    v5, _ = metrics_of(report, "V5")
    v6, _ = metrics_of(report, "V6")
    v1, _ = metrics_of(report, "V1")
    v12, _ = metrics_of(report, "V12")
    v13, _ = metrics_of(report, "V13")
    errors = [v for k, v in v5.items() if k.endswith("_volume_error")]
    tets = int(v12.get("tets", v1.get("tets", 0)))
    area = v13.get("input_surface_area", 0.0)
    return {
        "case": case,
        "tets": tets,
        "V1": status_of(report, "V1"),
        "V3": status_of(report, "V3"),
        "V4": status_of(report, "V4"),
        "V5": status_of(report, "V5"),
        "V6": status_of(report, "V6"),
        "V9": status_of(report, "V9"),
        "V13": status_of(report, "V13"),
        "misattr": int(v5.get("misattributed_cells", -1)),
        "adjacency": int(v6.get("region_adjacency_violations", -1)),
        "undecl": int(v6.get("undeclared_boundary_faces", -1)),
        "same_cell": int(v6.get("undeclared_boundary_same_cell", -1)),
        "undecl_area": v6.get("undeclared_boundary_area", 0.0),
        "vol_err": max(errors) if errors else 0.0,
        # --- P3, from [V13] ---
        "on_surface": v13.get("on_surface_area_frac", float("nan")),
        "dev_mean_h": v13.get("deviation_mean_frac_h", float("nan")),
        "dev_max_h": v13.get("deviation_max_frac_h", float("nan")),
        "offset_h": v13.get("offset_mean_frac_h", float("nan")),
        "disp_share": v13.get("displacement_share", float("nan")),
        "chord_mean_h": v13.get("chord_mean_frac_h", float("nan")),
        # --- P-2.1, read off the DELIVERED file ---
        "delivered_ok": (
            deliverable["exists"]
            and deliverable["tets_only"]
            and deliverable["box_only"]
            and deliverable["region"]
        ),
        "delivered": deliverable,
        "bnd_faces": int(v13.get("material_boundary_faces", -1)),
        # --- P2, from [V12] + [V13] ---
        "input_area": area,
        "tets_per_area": tets / area if area > 0 else float("nan"),
        "cells": int(v12.get("lattice_cells", -1)),
        "tets_per_cell": v12.get("tets_per_lattice_cell", float("nan")),
        "fan_tets": int(v12.get("tets_provenance_junction", -1)),
        "fan_cells": int(v12.get("cells_provenance_junction", -1)),
        "fan_per_cell": v12.get("tets_per_cell_junction", float("nan")),
        "cut_tets": int(v12.get("tets_provenance_cut", -1)),
        "cut_per_cell": v12.get("tets_per_cell_cut", float("nan")),
        "lattice_tets": int(v12.get("tets_provenance_lattice", -1)),
        "lattice_per_cell": v12.get("tets_per_cell_lattice", float("nan")),
    }


def table(rows, title, note, columns):
    """Print one goal property's table. `columns` is (header, width, formatter)."""
    print()
    print(title)
    print(note)
    header = " ".join(f"{name:>{width}}" for name, width, _ in columns)
    print(header)
    print("-" * len(header))
    for r in rows:
        if "error" in r:
            print(f"{r['case']:5} ERROR: {r['error']}")
            continue
        print(" ".join(f"{fmt(r):>{width}}" for _, width, fmt in columns))


def num(value, spec, scale=1.0):
    """Format a metric, printing `-` for the sentinel the verifier uses for 'absent'."""
    if value is None or value != value or value < 0:
        return "-"
    value = value * scale
    return format(int(round(value)) if spec == "d" else value, spec)


def main():
    os.makedirs(WORK, exist_ok=True)
    argv = list(sys.argv[1:])
    baseline = None
    if "--json" in argv:
        at = argv.index("--json")
        baseline = argv[at + 1]
        del argv[at : at + 2]
    cases = [c for c in CASES if not argv or c[0] in argv]

    rows = []
    for case, stls, overrides in cases:
        r = run(case, stls, overrides)
        rows.append(r)
        print(f"[{case}] done", file=sys.stderr)
        sys.stderr.flush()

    table(
        rows,
        "P3 - exact surfaces: the material boundary against the input surface ([V13])",
        "Read at the face CORNERS: on% is the share of boundary area anchored to the "
        "surface, and P3 is met only at 100.000. dev/h, off/h: area-weighted mean "
        "|distance| and SIGNED distance, per local edge. disp: |offset|/deviation - 0 rough "
        "about the right place, 1 a sheet in the wrong one. chord/h is the sag of an "
        "anchored flat facet across curvature - it falls as h^2, it is P2's business, and "
        "it is NOT a P3 violation.",
        [
            ("case", 5, lambda r: r["case"]),
            ("V13", 6, lambda r: r["V13"]),
            ("faces", 9, lambda r: num(r["bnd_faces"], "d")),
            ("on%", 9, lambda r: num(r["on_surface"], ".3f", 100.0)),
            ("dev/h%", 8, lambda r: num(r["dev_mean_h"], ".2f", 100.0)),
            ("max/h%", 8, lambda r: num(r["dev_max_h"], ".1f", 100.0)),
            ("off/h%", 8, lambda r: format(r["offset_h"] * 100.0, "+.2f")),
            # The share is |offset|/deviation, so on a mesh that is entirely on the surface
            # it is 0/0 and reads 1.000 from float noise. Blank it there: with nothing off
            # the surface there is no error to attribute.
            (
                "disp",
                6,
                lambda r: "-" if r["on_surface"] >= 1.0 else num(r["disp_share"], ".3f"),
            ),
            ("chord/h%", 9, lambda r: num(r["chord_mean_h"], ".2f", 100.0)),
        ],
    )

    table(
        rows,
        "P2 - minimum elements: the count, normalised and attributed ([V12] + [V13])",
        "tets/A: elements per unit INPUT surface area - the denominator both meshers see "
        "identically. cells: S5 lattice cells, which are TETS, so an untouched one emits "
        "exactly 1 and every t/cell below reads as what that path costs over leaving the "
        "cell alone. 'fan' is the escalated cells the conforming centroid fan owns.",
        [
            ("case", 5, lambda r: r["case"]),
            ("tets", 10, lambda r: num(r["tets"], "d")),
            ("tets/A", 10, lambda r: num(r["tets_per_area"], ".0f")),
            ("cells", 9, lambda r: num(r["cells"], "d")),
            ("t/cell", 7, lambda r: num(r["tets_per_cell"], ".2f")),
            ("fan tets", 9, lambda r: num(r["fan_tets"], "d")),
            ("fan%", 6, lambda r: num(r["fan_tets"] / r["tets"] if r.get("tets") else -1, ".1f", 100.0)),
            ("fan/cell", 8, lambda r: num(r["fan_per_cell"], ".2f")),
            ("cut/cell", 8, lambda r: num(r["cut_per_cell"], ".2f")),
            ("bg/cell", 7, lambda r: num(r["lattice_per_cell"], ".2f")),
        ],
    )

    table(
        rows,
        "P-2.1 - the delivered file IS the mesh (R4)",
        "Read off `<case>_s08_cut.vtu` alone, never the `_contract.vtu` beside it. "
        "tets-only: nothing in the file but VTK_TETRA. box-only: [V1]/[V3] fire nothing, so "
        "every free face is on a domain plane, none is shared by more than two tets, and no "
        "edge is non-manifold - which on an untagged document is exactly 'the only feature "
        "edges are the domain box'. region: identity rides on a cell array.",
        [
            ("case", 5, lambda r: r["case"]),
            ("delivered", 10, lambda r: "yes" if r["delivered"]["exists"] else "MISSING"),
            ("cells", 10, lambda r: num(r["delivered"]["cells"], "d")),
            ("tets-only", 10, lambda r: "yes" if r["delivered"]["tets_only"] else "NO"),
            ("box-only", 9, lambda r: "yes" if r["delivered"]["box_only"] else "NO"),
            ("region", 7, lambda r: "yes" if r["delivered"]["region"] else "NO"),
            ("R4", 4, lambda r: "OK" if r["delivered_ok"] else "FAIL"),
            ("fired", 40, lambda r: ", ".join(r["delivered"]["codes"]) or "-"),
        ],
    )

    table(
        rows,
        "P1/P4 - the mesh is produced and is usable: check statuses and the old metrics",
        "unchanged from the previous table; kept so a regression in one property is visible "
        "beside a gain in another.",
        [
            ("case", 5, lambda r: r["case"]),
            ("V1", 5, lambda r: r["V1"]),
            ("V3", 5, lambda r: r["V3"]),
            ("V4", 5, lambda r: r["V4"]),
            ("V5", 5, lambda r: r["V5"]),
            ("V6", 5, lambda r: r["V6"]),
            ("V9", 5, lambda r: r["V9"]),
            ("misattr", 8, lambda r: num(r["misattr"], "d")),
            ("adjac", 6, lambda r: num(r["adjacency"], "d")),
            ("undecl", 7, lambda r: num(r["undecl"], "d")),
            ("vol%", 7, lambda r: num(r["vol_err"], ".3f", 100.0)),
        ],
    )

    if baseline:
        with open(baseline, "w") as f:
            json.dump(rows, f, indent=2, sort_keys=True)
        print(f"\nwrote {baseline}")


if __name__ == "__main__":
    main()
