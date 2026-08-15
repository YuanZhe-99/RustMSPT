#!/usr/bin/env python3
"""Gate P-3.1's first question: what can the §6 cut table not express, and what does each
gap actually cost P3?

Gate G6-0 adopted the conforming centroid fan for every cell the frozen §6 table cannot
take. Re-opening that decision needs the gaps enumerated *and weighted*, because the
answer differs per gap - a new table row, a fix upstream in §5.2/S7, or genuine local
recovery with Steiner insertion - and design effort has to go where the surface damage is
rather than where the cell count is. A reason that escalates thousands of cells and leaves
the material boundary on the surface is not the one to work on.

Reads the verifier reports the acceptance runner already wrote; it re-meshes nothing.
Run `run_acceptance.py` first, with `RUSTMSPT_CUT_DIAG` set (it always is), so that
`escalation_reason` is in the mesh and `[V12]`/`[V13]` can break the numbers out.

Usage:
    python3 data/fixtures/meshgen/acceptance/run_acceptance.py
    python3 data/fixtures/meshgen/acceptance/escalation_census.py
"""

import json
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
WORK = os.path.join(ROOT, "data", "output", "acceptance")

# `Escalation`'s ordinals, in declaration order (src/meshgen/cut.rs), with what each one
# means for the table. The last column is the question P-3.1 has to answer for it.
REASONS = [
    (0, "junction", "2+ active patches, or an intersection curve runs through the cell"),
    (1, "multi_crossing", "invariant K1: one component crosses an edge more than once"),
    (2, "inconsistent", "S6 and S7 disagree: an I-O edge with no crossing, or a crossing on an edge that does not straddle"),
    (3, "dry_run", "the guarded dry-run rejected the children (orientation, volume, or the volume sum)"),
    (4, "quality", "a prism or quad piece stayed below the §4.4 dihedral floor after the ladder"),
]

CASES = ["a1", "a2", "a3", "a4", "a6a", "a6b", "a7a", "a7b", "a8"]


def metrics_of(report, section):
    for s in report["sections"]:
        if s["id"] == section:
            return dict(s["metrics"])
    return {}


def load(case):
    path = os.path.join(WORK, case + ".json")
    if not os.path.exists(path):
        return None
    with open(path) as f:
        report = json.load(f)
    v12 = metrics_of(report, "V12")
    v13 = metrics_of(report, "V13")
    if not any(k.startswith("escalated_tets_") for k in v12):
        return None
    return {"case": case, "v12": v12, "v13": v13}


def main():
    rows = [r for r in (load(c) for c in (sys.argv[1:] or CASES)) if r]
    if not rows:
        sys.exit(
            "no reports with `escalation_reason` found under "
            f"{WORK}\nrun run_acceptance.py first (it sets RUSTMSPT_CUT_DIAG)"
        )

    print("Per case: where the escalated elements are, and what they cost the surface")
    print(
        "off-area is material-boundary area whose face is NOT anchored to the input "
        "surface, charged to the lowest-numbered escalation reason among the face's two "
        "owners so nothing is double counted. `(none)` is the area no escalated cell "
        "touches - P3 damage the §6 table produced on its own."
    )
    header = (
        f"{'case':5} {'reason':16} {'cells':>8} {'tets':>9} {'t/cell':>7} "
        f"{'bnd area':>10} {'off area':>10} {'off %':>7} {'share of off':>13}"
    )
    print(header)
    print("-" * len(header))
    totals = {}
    for r in rows:
        v12, v13 = r["v12"], r["v13"]
        total_off = sum(
            v for k, v in v13.items() if k.endswith("_off_surface_area")
        )
        buckets = [(code, name) for code, name, _ in REASONS] + [(-1, "(none)")]
        for code, name in buckets:
            area = v13.get(f"escalation_{code}_boundary_area", 0.0)
            off = v13.get(f"escalation_{code}_off_surface_area", 0.0)
            cells = int(v12.get(f"escalated_cells_{name}", 0))
            tets = int(v12.get(f"escalated_tets_{name}", 0))
            if area == 0.0 and tets == 0:
                continue
            t = totals.setdefault(name, [0, 0, 0.0, 0.0])
            t[0] += cells
            t[1] += tets
            t[2] += area
            t[3] += off
            print(
                f"{r['case']:5} {name:16} {cells:>8,} {tets:>9,} "
                f"{tets / cells if cells else 0:>7.2f} {area:>10.5f} {off:>10.5f} "
                f"{100.0 * off / area if area else 0:>6.1f}% "
                f"{100.0 * off / total_off if total_off else 0:>12.1f}%"
            )
        print("-" * len(header))

    print()
    print("Summed over the nine cases - the ranking P-3.1 designs against")
    header = (
        f"{'reason':16} {'cells':>9} {'tets':>10} {'t/cell':>7} {'bnd area':>10} "
        f"{'off area':>10} {'off %':>7} {'share of off':>13}"
    )
    print(header)
    print("-" * len(header))
    grand_off = sum(t[3] for t in totals.values())
    order = sorted(totals.items(), key=lambda kv: -kv[1][3])
    for name, (cells, tets, area, off) in order:
        print(
            f"{name:16} {cells:>9,} {tets:>10,} {tets / cells if cells else 0:>7.2f} "
            f"{area:>10.5f} {off:>10.5f} {100.0 * off / area if area else 0:>6.1f}% "
            f"{100.0 * off / grand_off if grand_off else 0:>12.1f}%"
        )

    print()
    print("What each reason is:")
    for _, name, meaning in REASONS:
        print(f"  {name:16} {meaning}")


if __name__ == "__main__":
    main()
