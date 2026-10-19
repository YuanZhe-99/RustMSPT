#!/usr/bin/env python3
"""Performance envelope (plan M-6.5, GT-5): time and peak memory per stage per case.

Reads the mesher logs a `run_acceptance.py` / `run_reference.py` work directory keeps
(`<case>.mesh.log`, written with RUSTMSPT_TIME_STAGES=1) and prints, per case: tets, total stage
time, the three slowest stages, peak resident set and peak bytes per tet, each checked against the
plan's B.4 envelope (mesh state ~44 B/tet + 72 B/node; 20 M tets nominal) and M-1.3's time budgets.

    python3 data/fixtures/meshgen/acceptance/perf_envelope.py WORKDIR [WORKDIR ...] [--json out.json]

Times are wall times of the run as logged; a matrix run overlaps three cases, so for a budget
comparison re-run the case alone on an idle host.
"""

import glob
import json
import os
import re
import sys

UNIT = {"ns": 1e-9, "µs": 1e-6, "us": 1e-6, "ms": 1e-3, "s": 1.0}
# M-1.3's recorded budgets (gated path, seconds, idle host).
TIME_BUDGET = {"a3": 16.6, "a4": 83.8, "a8": 125.1}
# B.4: about 1.5 GB of mesh state at 20 M tets; the whole process is held to 4x that per tet.
BYTES_PER_TET_ENVELOPE = 4 * 1.5e9 / 20e6


def seconds(text):
    m = re.match(r"([\d.]+)(ns|µs|us|ms|s)$", text)
    return float(m.group(1)) * UNIT[m.group(2)] if m else 0.0


def case_row(log_path):
    stages, peak, tets = {}, 0, None
    with open(log_path, errors="ignore") as f:
        for line in f:
            m = re.match(r"\[STAGE-TIME\] (\S+) (\S+)", line)
            if m:
                stages[m.group(1)] = stages.get(m.group(1), 0.0) + seconds(m.group(2))
            m = re.match(r"\[STAGE-RSS\] \S+ (\d+)", line)
            if m:
                peak = max(peak, int(m.group(1)))
            m = re.search(r"\[mesh\] delivered: .* \((\d+) tets\)", line)
            if m:
                tets = int(m.group(1))
    if not stages:
        return None
    total = sum(stages.values())
    top = sorted(stages.items(), key=lambda kv: -kv[1])[:3]
    per_tet = peak / tets if tets else None
    return {
        "case": os.path.basename(log_path).split(".")[0],
        "tets": tets,
        "total_s": total,
        "top": top,
        "peak_mb": peak / 1e6,
        "bytes_per_tet": per_tet,
        "memory_ok": per_tet is None or per_tet <= BYTES_PER_TET_ENVELOPE,
    }


def main():
    args = sys.argv[1:]
    out = None
    if "--json" in args:
        i = args.index("--json")
        out = args[i + 1]
        del args[i : i + 2]
    rows = []
    for work in args:
        for log in sorted(glob.glob(os.path.join(work, "*.mesh.log")) + glob.glob(os.path.join(work, "*", "*.mesh.log"))):
            row = case_row(log)
            if row:
                rows.append(row)
    print(f"{'case':20s} {'tets':>10s} {'total s':>8s} {'peak MB':>8s} {'B/tet':>7s}  slowest stages")
    for r in rows:
        bpt = f"{r['bytes_per_tet']:.0f}" if r["bytes_per_tet"] else "-"
        flag = "" if r["memory_ok"] else "  OVER memory envelope"
        budget = TIME_BUDGET.get(r["case"])
        if budget is not None:
            flag += f"  (M-1.3 budget {budget:.1f} s: {'within' if r['total_s'] <= budget else 'over'})"
        top = ", ".join(f"{k} {v:.1f}" for k, v in r["top"])
        print(f"{r['case']:20s} {r['tets'] or 0:>10d} {r['total_s']:>8.1f} {r['peak_mb']:>8.0f} {bpt:>7s}  {top}{flag}")
    print(f"envelope: peak resident set <= {BYTES_PER_TET_ENVELOPE:.0f} B/tet (B.4: 1.5 GB of mesh state at 20 M tets, 4x for the process)")
    if out:
        with open(out, "w") as f:
            json.dump(rows, f, indent=1)


if __name__ == "__main__":
    main()
