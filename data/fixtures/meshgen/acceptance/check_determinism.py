#!/usr/bin/env python3
"""R-P2 on the acceptance matrix (plan M-1.2): the same case meshed at one worker and at the
default worker count must produce byte-identical files.

Reuses the per-case configs `run_acceptance.py` wrote under `<work>/<path>/`, re-runs each at
`RAYON_NUM_THREADS=1` into `<work>/<path>.t1/`, and compares the delivered volume and the contract
document of the two runs. The only lines excluded are the ones that name the run rather than the
mesh - the config hash and the output paths, which differ because the output directory does.

    python3 data/fixtures/meshgen/acceptance/check_determinism.py <work> [--path default|gated]
        [--out table.txt] [cases...]

Prints one row per case and exits non-zero on any mismatch.
"""
import argparse
import hashlib
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
BIN = os.environ.get("RUSTMSPT_BIN", os.path.join(ROOT, "target", "release", "rustmspt"))
NAMING = re.compile(rb"ConfigHash|<DataArray[^>]*Name=\"(OutputPath|ConfigPath|SourcePath)")


def digest(path):
    """SHA-256 of the file with the run-naming lines removed."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for line in f:
            if NAMING.search(line):
                continue
            h.update(line)
    return h.hexdigest()


def outputs(config_text):
    """The delivered VTU path a config names, and its contract companion."""
    m = re.search(r"(?m)^\s*vtu:\s*(\S+)", config_text)
    if not m:
        return []
    vtu = m.group(1)
    stem = vtu[:-4] if vtu.endswith(".vtu") else vtu
    case = os.path.basename(stem)
    debug = os.path.join(os.path.dirname(stem), case + ".debug")
    return [
        os.path.join(debug, f"{case}_s08_cut.vtu"),
        os.path.join(debug, f"{case}_s08_cut_contract.vtu"),
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("work")
    ap.add_argument("--path", default="gated", choices=["default", "gated"])
    ap.add_argument("--out")
    ap.add_argument("cases", nargs="*")
    args = ap.parse_args()

    src = os.path.join(os.path.abspath(args.work), args.path)
    dst = src + ".t1"
    os.makedirs(dst, exist_ok=True)
    cases = args.cases or sorted(
        f[:-5] for f in os.listdir(src) if f.endswith(".yaml") and not f.endswith("_verify.yaml")
    )
    # The same environment `run_acceptance.py` meshes under: its diagnostic arrays are in the
    # files being compared, so leaving them out would compare two different documents.
    env = dict(os.environ, RUSTMSPT_CUT_DIAG="1", RUSTMSPT_TIME_STAGES="1")
    env.pop("RUSTMSPT_PLC_PASS", None)
    if args.path == "gated":
        env["RUSTMSPT_PLC_PASS"] = "1"
        env["RUSTMSPT_PLC_DIAG"] = "1"
    rows = []
    failed = False
    for case in cases:
        text = open(os.path.join(src, case + ".yaml")).read()
        one = text.replace(src + "/", dst + "/")
        cfg = os.path.join(dst, case + ".yaml")
        with open(cfg, "w") as f:
            f.write(one)
        run = subprocess.run([BIN, "mesh", "--config", cfg], env={**env, "RAYON_NUM_THREADS": "1"},
                             capture_output=True, text=True)
        verdicts = []
        for many, single in zip(outputs(text), outputs(one)):
            if not (os.path.exists(many) and os.path.exists(single)):
                verdicts.append(("missing", "", ""))
                continue
            a, b = digest(many), digest(single)
            verdicts.append(("same" if a == b else "DIFF", a[:16], b[:16]))
        ok = verdicts and all(v[0] == "same" for v in verdicts)
        failed |= not ok
        rows.append((case, verdicts, run.returncode))
        print(f"{case:5s} " + "  ".join(f"{v[0]:4s} {v[1]}" for v in verdicts), flush=True)
    if args.out:
        with open(args.out, "w") as f:
            f.write(f"# R-P2, {args.path} path: default worker count against RAYON_NUM_THREADS=1\n")
            f.write("# case  delivered-volume  contract-document  (sha256 prefix, run-naming lines excluded)\n")
            for case, verdicts, _ in rows:
                f.write(f"{case:5s} " + "  ".join(f"{v[0]} {v[1]}" for v in verdicts) + "\n")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
