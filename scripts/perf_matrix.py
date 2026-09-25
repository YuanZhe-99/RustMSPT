#!/usr/bin/env python3
"""CPU worker-count benchmark matrix for the rustmspt pipelines (PERF-00).

Runs each selected pipeline at several worker counts, repeating every
configuration, and parses the `[Timing] <pipeline> stage=<name> seconds=<f>`,
`[Timing] <pipeline> workers=<n>` and `[Timing] <pipeline> peak_rss_bytes=<n>`
lines the binary prints. Every run gets a private config copy and a private
output directory under the work directory, so nothing under data/ is
overwritten. The first run of each (case, workers) pair is reported as the
cold run and excluded from the warm statistics; "cold" means first process of
that configuration, not a dropped page cache (that needs root).

Outputs: <work>/perf_matrix_raw.json and <work>/perf_matrix_summary.md.

Standard library only.
"""

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TIMING_STAGE = re.compile(r"^\[Timing\] (\S+) stage=(\S+) seconds=([-+0-9.eE]+)\s*$")
TIMING_WORKERS = re.compile(r"^\[Timing\] (\S+) workers=(\d+)\s*$")
TIMING_RSS = re.compile(r"^\[Timing\] (\S+) peak_rss_bytes=(\d+|unavailable)\s*$")
GRID_STATS = re.compile(r"^\[GridStats\] .*$")


def yaml_scalar(value):
    """Render a Python value as a YAML flow scalar/sequence (JSON is valid YAML)."""
    if isinstance(value, bool):
        return "true" if value else "false"
    return json.dumps(value)


def set_yaml_key(text, dotted, value):
    """Set `a.b.c` in a simple block-mapping YAML document, inserting it when absent.

    Handles the flat, indentation-structured configs shipped in data/input
    (comments preserved, flow values replaced whole). Not a general YAML editor.
    """
    keys = dotted.split(".")
    lines = text.split("\n")
    start, end, indent = 0, len(lines), -1
    for depth, key in enumerate(keys):
        found = None
        child_indent = None
        for i in range(start, end):
            line = lines[i]
            stripped = line.lstrip(" ")
            if not stripped or stripped.startswith("#"):
                continue
            ind = len(line) - len(stripped)
            if ind <= indent:
                end = i
                break
            if child_indent is None:
                child_indent = ind
            if ind != child_indent:
                continue
            if re.match(r"^" + re.escape(key) + r"\s*:", stripped):
                found = i
                break
        if child_indent is None:
            child_indent = indent + 2 if indent >= 0 else 0
        if found is None:
            insert_at = start
            rest = keys[depth:]
            new_lines = []
            for offset, k in enumerate(rest):
                pad = " " * (child_indent + 2 * offset)
                if offset == len(rest) - 1:
                    new_lines.append(f"{pad}{k}: {yaml_scalar(value)}")
                else:
                    new_lines.append(f"{pad}{k}:")
            lines[insert_at:insert_at] = new_lines
            return "\n".join(lines)
        if depth == len(keys) - 1:
            pad = " " * child_indent
            lines[found] = f"{pad}{key}: {yaml_scalar(value)}"
            j = found + 1
            while j < len(lines):
                stripped = lines[j].lstrip(" ")
                if stripped and not stripped.startswith("#") and len(lines[j]) - len(stripped) <= child_indent:
                    break
                j += 1
            del lines[found + 1 : j]
            return "\n".join(lines)
        start, indent = found + 1, child_indent
        end = len(lines)
        for i in range(start, len(lines)):
            stripped = lines[i].lstrip(" ")
            if stripped and not stripped.startswith("#") and len(lines[i]) - len(stripped) <= indent:
                end = i
                break
    return "\n".join(lines)


def case_definitions(repo, chain, large=False):
    """Return the benchmark cases. Each case builds (config_text, extra_args) for one run directory.

    `large` raises the work per run so worker scaling is measurable: the shipped inputs finish most
    pipelines in 3-90 ms, which measures process and pool start-up rather than scaling
    (PLAN.Performance.md section 68). It changes only sizes and targets, never a method.
    """
    inp = repo / "data" / "input"

    def load(name):
        return (inp / name).read_text()

    def split_filter(run, workers):
        t = load("split_filter_config.yaml")
        t = set_yaml_key(t, "cpu_max", workers)
        t = set_yaml_key(t, "input.path", str(chain / "dense_particles.stl" if large else inp / "particles.stl"))
        t = set_yaml_key(t, "output.folder", str(run / "split"))
        t = set_yaml_key(t, "output.report_path", str(run / "split_filter_report.txt"))
        return t, []

    def pack(run, workers):
        t = load("pack_config.yaml")
        t = set_yaml_key(t, "packing.cpu_max", workers)
        t = set_yaml_key(t, "input.path", str(inp / "particles.stl"))
        t = set_yaml_key(t, "packing.target_diameter_distribution_csv", str(inp / "gu2019_fig7b_pore_distribution.csv"))
        t = set_yaml_key(t, "output.path", str(run / "packed_result.stl"))
        if large:
            t = set_yaml_key(t, "packing.target_volume_fraction", 0.15)
            t = set_yaml_key(t, "packing.max_attempts", 20000)
        return t, []

    def placement(run, workers):
        t = load("placement_config.yaml")
        t = set_yaml_key(t, "placement.shapes.files", [str(inp / "particles.stl")])
        t = set_yaml_key(t, "placement.outputs.dir", str(run / "placement"))
        if large:
            t = set_yaml_key(t, "placement.target.volume_fraction", 0.30)
        return t, ["--threads", str(workers)]

    def optimize(run, workers, iterations):
        t = load("optimize_config.yaml")
        t = set_yaml_key(t, "optimization.cpu_max", workers)
        t = set_yaml_key(t, "input.stl_path", str(chain / "packed_result.stl"))
        t = set_yaml_key(t, "output.path", str(run / "optimized_structure.stl"))
        if iterations is not None:
            t = set_yaml_key(t, "optimization.max_iterations", iterations)
        return t, []

    def measure(run, workers):
        t = load("measure_config.yaml")
        t = set_yaml_key(t, "measurement.cpu_max", workers)
        t = set_yaml_key(t, "measurement.stl_path", str(chain / "optimized_structure.stl"))
        t = set_yaml_key(t, "measurement.output_path", str(run / "measured_s2.txt"))
        if large:
            t = set_yaml_key(t, "measurement.voxel_pitch", 0.5)
            t = set_yaml_key(t, "measurement.mc_samples", 400000)
        return t, []

    def forge(run, workers):
        t = load("forge_config.yaml")
        t = set_yaml_key(t, "forging.input_stl_path", str(chain / "dense_particles.stl" if large else chain / "optimized_structure.stl"))
        t = set_yaml_key(t, "forging.output_stl_path", str(run / "forged_mesh.stl"))
        return t, []

    def scale(run, workers):
        t = load("scale_config.yaml")
        t = set_yaml_key(t, "input.stl_path", str(chain / "dense_particles.stl" if large else chain / "optimized_structure.stl"))
        t = set_yaml_key(t, "output.stl_path", str(run / "scaled_mesh.stl"))
        return t, []

    def render(run, workers):
        t = load("render_config.yaml")
        t = set_yaml_key(t, "render.cpu_max", workers)
        t = set_yaml_key(t, "render.stl_path", str(inp / "particles.stl"))
        t = set_yaml_key(t, "render.output_path", str(run / "rendered.png"))
        if large:
            t = set_yaml_key(t, "render.width", 4096)
            t = set_yaml_key(t, "render.height", 4096)
        return t, []

    def mesh_render(run, workers):
        t = load("mesh_render_config.yaml")
        t = set_yaml_key(t, "cpu_max", workers)
        t = set_yaml_key(t, "mesh_render.input", str(repo / "data" / "fixtures" / "meshgen" / "good_cube.vtu"))
        t = set_yaml_key(t, "mesh_render.output_dir", str(run / "mesh_render"))
        if large:
            t = set_yaml_key(t, "mesh_render.width", 3072)
            t = set_yaml_key(t, "mesh_render.height", 3072)
        return t, []

    def crop(run, workers):
        t = load("crop_config.yaml")
        t = set_yaml_key(t, "cpu_max", workers)
        t = set_yaml_key(t, "input.path", str(inp / "ct_stack"))
        t = set_yaml_key(t, "output.path", str(run / "cropped_ct.tiff"))
        return t, []

    def dense_source(run, workers):
        t = load("placement_config.yaml")
        t = set_yaml_key(t, "placement.shapes.files", [str(inp / "particles.stl")])
        t = set_yaml_key(t, "placement.domain.max", [200, 200, 200])
        t = set_yaml_key(t, "placement.target.volume_fraction", 0.30)
        t = set_yaml_key(t, "placement.outputs.dir", str(run / "placement"))
        return t, ["--threads", str(workers)]

    return {
        "dense-source": ("pack", dense_source, False),
        "split-filter": ("split-filter", split_filter, False),
        "pack": ("pack", pack, False),
        "placement": ("pack", placement, False),
        "optimize": ("optimize", optimize, True),
        "measure": ("measure", measure, True),
        "forge": ("forge", forge, True),
        "scale": ("scale", scale, True),
        "render": ("render", render, False),
        "mesh-render": ("mesh-render", mesh_render, False),
        "crop": ("crop", crop, False),
    }


def run_once(binary, repo, subcommand, config_text, extra_args, run_dir, workers, timeout):
    """Run one pipeline process in its own directory; return the parsed record."""
    run_dir.mkdir(parents=True, exist_ok=True)
    config = run_dir / "config.yaml"
    config.write_text(config_text)
    env = dict(os.environ)
    env["RAYON_NUM_THREADS"] = str(workers)
    env["RUSTMSPT_ACCELERATION"] = "cpu"
    cmd = [str(binary), subcommand, "--config", str(config)] + extra_args
    started = time.perf_counter()
    proc = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=timeout)
    wall = time.perf_counter() - started
    (run_dir / "stdout.log").write_text(proc.stdout)
    (run_dir / "stderr.log").write_text(proc.stderr)
    record = {
        "command": cmd,
        "returncode": proc.returncode,
        "process_wall_seconds": wall,
        "stages": {},
        "workers_reported": None,
        "peak_rss_bytes": None,
        "grid_stats": [],
    }
    for line in proc.stdout.splitlines():
        m = TIMING_STAGE.match(line)
        if m:
            record["stages"][m.group(2)] = record["stages"].get(m.group(2), 0.0) + float(m.group(3))
            continue
        m = TIMING_WORKERS.match(line)
        if m:
            record["workers_reported"] = int(m.group(2))
            continue
        m = TIMING_RSS.match(line)
        if m:
            record["peak_rss_bytes"] = None if m.group(2) == "unavailable" else int(m.group(2))
            continue
        if GRID_STATS.match(line):
            record["grid_stats"].append(line)
    return record


def summarize(results, worker_counts):
    """Build the markdown summary from the raw run records."""
    out = ["# Performance matrix", ""]
    out.append("Warm statistics exclude the cold (warmup) runs of each case/worker pair; the cold column is the first of them. "
               "S(p) = median T(1) / median T(p); E(p) = S(p) / reported workers. "
               "`process_wall` is the whole process as seen by the runner.")
    out.append("")
    for case, per_workers in results["cases"].items():
        out.append(f"## {case}")
        out.append("")
        failures = [r for runs in per_workers.values() for r in runs if r["returncode"] != 0]
        if failures:
            out.append(f"**{len(failures)} run(s) failed** (see raw JSON / stderr.log).")
            out.append("")
        stages = []
        for runs in per_workers.values():
            for r in runs:
                for s in list(r["stages"]) + ["process_wall"]:
                    if s not in stages:
                        stages.append(s)
        out.append("| stage | requested p | workers | cold s | warm median s | min s | max s | S(p) | E(p) | peak RSS MiB (median) |")
        out.append("|---|---|---|---|---|---|---|---|---|---|")
        base = {}
        for stage in stages:
            for p in worker_counts:
                runs = [r for r in per_workers.get(str(p), []) if r["returncode"] == 0]
                if not runs:
                    continue

                def value(r):
                    return r["process_wall_seconds"] if stage == "process_wall" else r["stages"].get(stage)

                colds = [value(r) for r in runs if r.get("cold") and value(r) is not None]
                cold = colds[0] if colds else None
                warm_runs = [r for r in runs if not r.get("cold")]
                warm = [value(r) for r in warm_runs if value(r) is not None]
                if not warm:
                    continue
                med = statistics.median(warm)
                if p == worker_counts[0]:
                    base[stage] = med
                reported = runs[-1]["workers_reported"] or p
                speedup = base.get(stage, float("nan")) / med if med > 0 and stage in base else float("nan")
                eff = speedup / reported if reported else float("nan")
                rss = [r["peak_rss_bytes"] for r in warm_runs if r["peak_rss_bytes"] is not None]
                rss_txt = f"{statistics.median(rss) / 2**20:.1f}" if rss else "n/a"
                cold_txt = f"{cold:.4f}" if cold is not None else "n/a"
                out.append(f"| {stage} | {p} | {reported} | {cold_txt} | {med:.4f} | {min(warm):.4f} | {max(warm):.4f} | {speedup:.2f} | {eff:.2f} | {rss_txt} |")
        grid = [line for runs in per_workers.values() for r in runs[:1] for line in r["grid_stats"]]
        if grid:
            out.append("")
            out.append("Grid statistics (first run of each worker count):")
            out.append("")
            out.extend(f"- `{line}`" for line in grid)
        out.append("")
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default="target/release/rustmspt", help="release binary (default: target/release/rustmspt)")
    parser.add_argument("--repo", default=str(Path(__file__).resolve().parent.parent), help="repository root (default: script's parent)")
    parser.add_argument("--work", default=None, help="work directory for configs, outputs and reports (default: a new temp dir)")
    parser.add_argument("--cases", default="split-filter,pack,placement,optimize,measure,forge,scale,render,mesh-render,crop")
    parser.add_argument("--workers", default="1,2,4,8", help="comma-separated worker counts")
    parser.add_argument("--warmup", type=int, default=1, help="cold runs per case/worker pair, reported separately")
    parser.add_argument("--repeats", type=int, default=5, help="warm runs per case/worker pair")
    parser.add_argument("--optimize-iterations", type=int, default=None, help="override optimization.max_iterations (reduced runs)")
    parser.add_argument("--timeout", type=float, default=3600.0, help="per-process timeout in seconds")
    parser.add_argument("--large", action="store_true", help="larger per-run work so worker scaling is measurable (sizes and targets only)")
    parser.add_argument("--chain", default=None, help="reuse an existing chain directory (packed_result.stl, optimized_structure.stl) instead of regenerating it; pack is unseeded, so matrices only compare optimize/measure on a shared chain")
    args = parser.parse_args()

    repo = Path(args.repo).resolve()
    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = (repo / binary).resolve()
    if not binary.exists():
        sys.exit(f"binary not found: {binary}")
    work = Path(args.work).resolve() if args.work else Path(tempfile.mkdtemp(prefix="perf_matrix_"))
    work.mkdir(parents=True, exist_ok=True)
    worker_counts = [int(w) for w in args.workers.split(",") if w.strip()]
    selected = [c.strip() for c in args.cases.split(",") if c.strip()]
    chain = Path(args.chain).resolve() if args.chain else work / "chain"
    cases = case_definitions(repo, chain, args.large)
    unknown = [c for c in selected if c not in cases]
    if unknown:
        sys.exit(f"unknown case(s): {unknown}; known: {sorted(cases)}")

    def build(case, run, workers):
        subcommand, builder, _ = cases[case]
        if case == "optimize":
            text, extra = builder(run, workers, args.optimize_iterations)
        else:
            text, extra = builder(run, workers)
        return subcommand, text, extra

    results = {
        "binary": str(binary),
        "repo": str(repo),
        "work": str(work),
        "worker_counts": worker_counts,
        "warmup": args.warmup,
        "repeats": args.repeats,
        "optimize_iterations": args.optimize_iterations,
        "large": args.large,
        "chain_reused": str(chain) if args.chain else None,
        "cpu_count": os.cpu_count(),
        "chain": {},
        "cases": {},
    }

    dense_cases = {"split-filter", "forge", "scale"}
    if args.large and dense_cases.intersection(selected) and not (chain / "dense_particles.stl").exists():
        chain.mkdir(parents=True, exist_ok=True)
        run = work / "chain_runs" / "dense-source"
        subcommand, text, extra = build("dense-source", run, os.cpu_count() or 1)
        print("[perf_matrix] chain: dense-source (seeded placement, 200^3 domain, VF 0.30)", flush=True)
        record = run_once(binary, repo, subcommand, text, extra, run, os.cpu_count() or 1, args.timeout)
        results["chain"]["dense-source"] = record
        if record["returncode"] != 0:
            sys.exit(f"chain step dense-source failed; see {run}/stderr.log")
        shutil.copyfile(run / "placement" / "particles.stl", chain / "dense_particles.stl")
    if args.chain:
        for artefact in ("packed_result.stl", "optimized_structure.stl"):
            if not (chain / artefact).exists():
                sys.exit(f"--chain {chain} has no {artefact}")
    elif any(cases[c][2] for c in selected if c not in dense_cases or not args.large):
        chain.mkdir(parents=True, exist_ok=True)
        for producer, artefact in (("pack", "packed_result.stl"), ("optimize", "optimized_structure.stl")):
            run = work / "chain_runs" / producer
            subcommand, text, extra = build(producer, run, os.cpu_count() or 1)
            print(f"[perf_matrix] chain: {producer}", flush=True)
            record = run_once(binary, repo, subcommand, text, extra, run, os.cpu_count() or 1, args.timeout)
            results["chain"][producer] = record
            if record["returncode"] != 0:
                sys.exit(f"chain step {producer} failed; see {run}/stderr.log")
            shutil.copyfile(run / artefact, chain / artefact)

    for case in selected:
        results["cases"][case] = {}
        for p in worker_counts:
            runs = []
            for index in range(args.warmup + args.repeats):
                run = work / "runs" / case / f"p{p}" / f"r{index}"
                subcommand, text, extra = build(case, run, p)
                print(f"[perf_matrix] {case} p={p} run {index + 1}/{args.warmup + args.repeats}", flush=True)
                record = run_once(binary, repo, subcommand, text, extra, run, p, args.timeout)
                record["cold"] = index < args.warmup
                runs.append(record)
                if run.exists():
                    for child in run.iterdir():
                        if child.is_dir():
                            shutil.rmtree(child)
                        elif child.suffix in (".stl", ".png", ".tiff", ".tif", ".raw", ".json", ".csv"):
                            child.unlink()
            results["cases"][case][str(p)] = runs

    raw = work / "perf_matrix_raw.json"
    raw.write_text(json.dumps(results, indent=2))
    summary = work / "perf_matrix_summary.md"
    summary.write_text(summarize(results, worker_counts))
    print(f"[perf_matrix] raw: {raw}")
    print(f"[perf_matrix] summary: {summary}")


if __name__ == "__main__":
    main()
