#!/usr/bin/env python3
"""P2 at equal fidelity (plan M-1.4): for each case, the default path's element count at the `h`
that reaches the gated path's on-surface share, beside the gated path's count at its own `h`.

Reads both paths' results from a finished `run_acceptance.py` work directory, then re-meshes each
case on the default path with `h_max_frac` and `h_min_frac` halved until its `[V13]` on-surface
share meets the gated one, or the run exceeds the memory cap (2 GB by default), or `--max-halvings`
is reached. Each attempt is verified with the case's own verify config.

    python3 data/fixtures/meshgen/acceptance/p2_equal_fidelity.py <work> [--out table.md]
        [--cap-mb 2048] [--max-halvings 3] [cases...]
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
BIN = os.environ.get("RUSTMSPT_BIN", os.path.join(ROOT, "target", "release", "rustmspt"))


def v13(report_path):
    """(on-surface share, tets) from a verify JSON report."""
    with open(report_path) as f:
        report = json.load(f)
    on, tets = None, None
    for section in report.get("sections", []):
        metrics = section.get("metrics", {})
        if on is None and "on_surface_area_frac" in metrics:
            on = metrics["on_surface_area_frac"]
        if tets is None and "tets" in metrics:
            tets = metrics["tets"]
    return on, tets


def scaled(text, factor, work_from, work_to):
    """The config with its resolution fractions scaled and its outputs redirected."""
    def scale(m):
        return f"{m.group(1)}{float(m.group(2)) * factor!r}"
    out = re.sub(r"(?m)^(\s*h_max_frac:\s*)([0-9.eE+-]+)", scale, text)
    out = re.sub(r"(?m)^(\s*h_min_frac:\s*)([0-9.eE+-]+)", scale, out)
    # The envelope scales with the element: `validate()` requires eps below half a sheet at
    # `h_min`, and an envelope left at the coarse size would also stop meaning "coincident".
    out = re.sub(r"(?m)^(\s*eps_frac:\s*)([0-9.eE+-]+)", scale, out)
    return out.replace(work_from, work_to)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--out")
    ap.add_argument("--cap-mb", type=int, default=2048)
    ap.add_argument("--max-halvings", type=int, default=3)
    ap.add_argument("cases", nargs="*")
    args = ap.parse_args()
    work = os.path.abspath(args.work)
    gated_dir, default_dir = os.path.join(work, "gated"), os.path.join(work, "default")
    cases = args.cases or sorted(
        f[:-5] for f in os.listdir(default_dir)
        if f.endswith(".yaml") and not f.endswith("_verify.yaml")
    )
    env = dict(os.environ)
    env.pop("RUSTMSPT_PLC_PASS", None)
    rows = []
    for case in cases:
        g_on, g_tets = v13(os.path.join(gated_dir, case + ".json"))
        d_on, d_tets = v13(os.path.join(default_dir, case + ".json"))
        best = (1.0, d_on, d_tets, "")
        base = open(os.path.join(default_dir, case + ".yaml")).read()
        vbase = open(os.path.join(default_dir, case + "_verify.yaml")).read()
        factor = 1.0
        note = "met at h" if d_on is not None and g_on is not None and d_on >= g_on else ""
        halvings = 0
        while not note and halvings < args.max_halvings:
            halvings += 1
            factor *= 0.5
            sub = os.path.join(work, f"p2_h{halvings}")
            os.makedirs(sub, exist_ok=True)
            cfg = os.path.join(sub, case + ".yaml")
            with open(cfg, "w") as f:
                f.write(scaled(base, factor, default_dir, sub))
            vcfg = os.path.join(sub, case + "_verify.yaml")
            with open(vcfg, "w") as f:
                f.write(vbase.replace(default_dir, sub))
            timed = subprocess.run(
                ["/usr/bin/time", "-f", "%M", BIN, "mesh", "--config", cfg],
                env=env, capture_output=True, text=True,
            )
            tail = timed.stderr.strip().splitlines() or ["0"]
            peak_mb = int(tail[-1]) / 1024 if tail[-1].isdigit() else 0.0
            report = os.path.join(sub, case + ".json")
            if os.path.exists(report):
                os.remove(report)
            if "leaf budget" in timed.stdout + timed.stderr:
                note = f"sizing leaf budget reached at h/{2 ** halvings}"
                break
            subprocess.run([BIN, "mesh-verify", "--config", vcfg], env=env, capture_output=True)
            if not os.path.exists(report):
                err = [l for l in (timed.stdout + timed.stderr).splitlines() if "rror" in l][:1]
                note = f"no mesh at h/{2 ** halvings}: {err[0][:80] if err else '?'}"
                break
            on, tets = v13(report)
            best = (factor, on, tets, f"{peak_mb:.0f} MB")
            print(f"{case} h/{2 ** halvings}: on {on} tets {tets} peak {peak_mb:.0f} MB", flush=True)
            if on is not None and g_on is not None and on >= g_on:
                note = f"met at h/{2 ** halvings}"
            elif peak_mb > args.cap_mb:
                note = f"cap reached at h/{2 ** halvings}"
        if not note:
            note = f"not met by h/{2 ** halvings}"
        rows.append((case, g_on, g_tets, d_on, d_tets, best, note))
    lines = [
        "| case | gated on % | gated tets | default on % at h | default tets at h | "
        "default on % at sweep end | default tets at sweep end | peak | outcome |",
        "|---|---|---|---|---|---|---|---|---|",
    ]
    pct = lambda x: "-" if x is None else f"{100 * x:.3f}"
    num = lambda x: "-" if x is None else f"{int(x):,}"
    for case, g_on, g_tets, d_on, d_tets, best, note in rows:
        lines.append(
            f"| {case} | {pct(g_on)} | {num(g_tets)} | {pct(d_on)} | {num(d_tets)} | "
            f"{pct(best[1])} | {num(best[2])} | {best[3]} | {note} |"
        )
    text = "\n".join(lines) + "\n"
    print(text)
    if args.out:
        with open(args.out, "w") as f:
            f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
