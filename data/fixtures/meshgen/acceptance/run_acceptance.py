#!/usr/bin/env python3
"""Run the nine acceptance cases end to end and print the metric table.

Why this exists: the cases' configs lived only in shell history. Every session that
measured them rebuilt the configs by hand, so a number quoted in one session could not
be reproduced in the next - and twice a conclusion was argued from a measurement that
no longer existed anywhere. The configs are the experiment; they belong in the repo.

Usage:
    python3 data/fixtures/meshgen/acceptance/run_acceptance.py [case ...]

With no arguments it runs all nine. `RUSTMSPT_CUT_DIAG` is set for every run so the
`parent_cell` array is present and the undeclared-boundary metric can separate a
defect *inside* one escalated cell from a disagreement between two.
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
    ("a6a", ["a6a_cube.stl", "a6a_limb.stl"], {"h_max_frac": 0.04, "h_min_frac": 0.002}),
    ("a6b", ["a6b_cube.stl", "a6b_limb.stl"], {"h_max_frac": 0.04, "h_min_frac": 0.002}),
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
    gap_cells: 2.0
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
    split_volume: false
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


def status_of(report, section):
    return metrics_of(report, section)[1]


def run(case, stls, overrides):
    config = write_config(case, stls, overrides)
    env = dict(os.environ, RUSTMSPT_CUT_DIAG="1")
    mesh = subprocess.run(
        [BIN, "mesh", "--config", config], capture_output=True, text=True, env=env
    )
    # `mesh` exits non-zero by design - S9..S11 are unimplemented - after writing the
    # s08 cut snapshot, which is the mesh these gates are measured on.
    vtu = os.path.join(WORK, case + ".debug", case + "_s08_cut.vtu")
    if not os.path.exists(vtu):
        return {"case": case, "error": [(mesh.stderr or mesh.stdout).strip()[-200:]]}
    js = os.path.join(WORK, case + ".json")
    vcfg = os.path.join(WORK, case + "_verify.yaml")
    with open(vcfg, "w") as f:
        f.write("mesh_verify:\n  input: {}\n  json: {}\n  surfaces:\n".format(vtu, js))
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
    v5, _ = metrics_of(report, "V5")
    v6, _ = metrics_of(report, "V6")
    v1, _ = metrics_of(report, "V1")
    errors = [v for k, v in v5.items() if k.endswith("_volume_error")]
    return {
        "case": case,
        "tets": int(v1.get("tets", 0)),
        "V1": status_of(report, "V1"),
        "V3": status_of(report, "V3"),
        "V5": status_of(report, "V5"),
        "V6": status_of(report, "V6"),
        "V9": status_of(report, "V9"),
        "misattr": int(v5.get("misattributed_cells", -1)),
        "adjacency": int(v6.get("region_adjacency_violations", -1)),
        "undecl": int(v6.get("undeclared_boundary_faces", -1)),
        "same_cell": int(v6.get("undeclared_boundary_same_cell", -1)),
        "undecl_area": v6.get("undeclared_boundary_area", 0.0),
        "vol_err": max(errors) if errors else 0.0,
    }


def main():
    os.makedirs(WORK, exist_ok=True)
    wanted = sys.argv[1:]
    cases = [c for c in CASES if not wanted or c[0] in wanted]
    header = (
        f"{'case':5} {'tets':>9} {'V1':>5} {'V3':>5} {'V5':>5} {'V6':>5} {'V9':>5} "
        f"{'misattr':>8} {'adjac':>6} {'undecl':>7} {'same':>7} {'vol%':>7}"
    )
    print(header)
    print("-" * len(header))
    for case, stls, overrides in cases:
        r = run(case, stls, overrides)
        if "error" in r:
            print(f"{case:5} ERROR: {r['error']}")
            sys.stdout.flush()
            continue
        print(
            f"{r['case']:5} {r['tets']:>9} {r['V1']:>5} {r['V3']:>5} {r['V5']:>5} "
            f"{r['V6']:>5} {r['V9']:>5} {r['misattr']:>8} {r['adjacency']:>6} "
            f"{r['undecl']:>7} {r['same_cell']:>7} {r['vol_err']*100:>7.3f}"
        )
        sys.stdout.flush()


if __name__ == "__main__":
    main()
