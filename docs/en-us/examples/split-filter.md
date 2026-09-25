# `split-filter` pipeline example

## What it does

`split-filter` takes one STL mesh containing multiple disjoint particles (for example, the merged
output of a forging or measurement step), splits it into one STL file per connected component, and
optionally filters out particles that fail geometric criteria (aspect ratio, sharpness ratio,
and/or a volume rule). It writes the surviving particles as individual STL files plus a text report
summarizing what was kept and removed.

## Config

`data/input/split_filter_config.yaml` (the repository's real default config for this pipeline):

```yaml
input:
  # Input STL file or folder containing STL files
  path: "data/input/particles.stl"

output:
  # Output is always a folder containing split particle STL files
  folder: "data/output/split_particles"
  # Output naming prefix, generated as: prefix + index + ".stl"
  # Example: "particle_" -> particle_1.stl, particle_2.stl, ...
  prefix: "particle_"
  # Optional report path. If omitted, defaults to: parent(<output.folder>)/split_filter_report.txt
  report_path: "data/output/split_filter_report.txt"

filter:
  # Set false to skip all filtering and only do splitting
  enabled: true

  # Optional geometric filters
  max_aspect_ratio: 3.0
  max_sharpness_ratio: 2.0

  volume:
    # Volume filtering mode:
    # - none: no volume filter
    # - range: keep particles in [min, max], -1 means no bound
    # - lognormal_rebalance: suppress overrepresented size ranges based on fitted lognormal profile
    mode: "range"

    # Used by mode=range
    min: 10.0
    max: -1

    # Used by mode=lognormal_rebalance
    bins: 12
    over_factor: 1.25
```

`input.path` points at `data/input/particles.stl`, a real 15-particle merged mesh checked into the
repository. For this walkthrough we keep the filter settings unchanged but copy the config to
`/tmp/rustmspt-doc-examples/split_filter_config_demo.yaml` with `output.report_path` redirected
under `/tmp/rustmspt-doc-examples/` (combined with a CLI `--output` override for the STL folder),
so the run writes nothing under `data/output/`.

| Field | Meaning |
|---|---|
| `filter.enabled` | When `false`, all particles are split out but none are removed regardless of the other `filter.*` settings. |
| `filter.max_aspect_ratio` | Upper bound on a particle's bounding-box aspect ratio (longest / shortest extent); particles above this are removed. |
| `filter.max_sharpness_ratio` | Upper bound on a particle's sharpness ratio (a mesh-shape spikiness metric); particles above this are removed. |
| `filter.volume.mode` | `"none"` disables the volume filter; `"range"` keeps particles with volume in `[min, max]`; `"lognormal_rebalance"` reduces overrepresented size bins based on a fitted lognormal size distribution instead of a hard cutoff. |
| `filter.volume.min`/`max` | Bounds used by `"range"` mode; `-1` for `max` means unbounded above. |
| `output.report_path` | Where the text report (particle counts, kept/removed histograms) is written; defaults next to `output.folder` if omitted. |

See `../reference/config.md` (`SplitFilterConfig` section) for the full field reference, and
`../reference/pipeline-crop-and-splitfilter.md` for the `SplitFilterPipeline::run` behavior this
example exercises (splitting, per-step filter accounting, and report generation).

## Running it

```bash
cp data/input/split_filter_config.yaml /tmp/rustmspt-doc-examples/split_filter_config_demo.yaml
# edit output.report_path in the copy to /tmp/rustmspt-doc-examples/split_filter_report.txt

./target/release/rustmspt split-filter \
  --config /tmp/rustmspt-doc-examples/split_filter_config_demo.yaml \
  --output /tmp/rustmspt-doc-examples/split_particles
```

`--output` overrides `output.folder` from the loaded YAML (see `src/main.rs`'s
`Commands::SplitFilter` arm); `--output` does not affect `output.report_path`, which is why the
scratch config also redirects `report_path` so the report lands under
`/tmp/rustmspt-doc-examples/` as well, keeping `data/output/` untouched.

## Expected output

Real captured stdout from the run above:

```
[Info] Split filter completed: input particles=15, kept=14, removed=1
[Info] Output folder: /tmp/rustmspt-doc-examples/split_particles
[Info] Output prefix: particle_
[Info] Report written: /tmp/rustmspt-doc-examples/split_filter_report.txt
```

The run completed in about 40 ms (`real 0m0.042s`). The output folder contains 14 files —
`particle_1.stl` through `particle_14.stl` — one per surviving particle.

Real content of `/tmp/rustmspt-doc-examples/split_filter_report.txt`:

```
Method: split_filter
Input path: data/input/particles.stl
Output folder: /tmp/rustmspt-doc-examples/split_particles
Report path: /tmp/rustmspt-doc-examples/split_filter_report.txt
Output prefix: particle_
Total split particles: 15

Filter steps:
initial: before=15, after=15, removed=0
max_aspect_ratio <= 3: before=15, after=14, removed=1
max_sharpness_ratio <= 2: before=14, after=14, removed=0
volume range filter (min=10, max=-1): before=14, after=14, removed=0

Summary:
Kept particles: 14
Removed particles: 1
Kept volume statistics: 
  min: 77.660666
  max: 815.672294
  mean: 349.195035
  median: 271.127167

Kept volume histogram (10 bins):
  [77.660666, 151.461829) |    3 | ##############################
  [151.461829, 225.262992) |    3 | ##############################
  [225.262992, 299.064155) |    2 | ####################
  [299.064155, 372.865317) |    1 | ##########
  [372.865317, 446.666480) |    0 | #
  [446.666480, 520.467643) |    2 | ####################
  [520.467643, 594.268806) |    1 | ##########
  [594.268806, 668.069969) |    0 | #
  [668.069969, 741.871131) |    0 | #
  [741.871131, 815.672294) |    2 | ####################

Volume histogram comparison (before vs after, 10 bins):
  [30.282125, 108.821142) | before    2 ########         | after    1 ####            
  [108.821142, 187.360159) | before    4 ################ | after    4 ################
  [187.360159, 265.899176) | before    2 ########         | after    2 ########        
  [265.899176, 344.438193) | before    2 ########         | after    2 ########        
  [344.438193, 422.977210) | before    0 #                | after    0 #               
  [422.977210, 501.516226) | before    1 ####             | after    1 ####            
  [501.516226, 580.055243) | before    1 ####             | after    1 ####            
  [580.055243, 658.594260) | before    1 ####             | after    1 ####            
  [658.594260, 737.133277) | before    0 #                | after    0 #               
  [737.133277, 815.672294) | before    2 ########         | after    2 ########        
```

Reading the report: of the 15 particles produced by splitting `particles.stl`, only the
`max_aspect_ratio <= 3` step actually removed anything (1 particle, an overly elongated shard);
neither `max_sharpness_ratio` nor the `volume` range filter (`min=10, max=-1`) removed any further
particles here, since the merged mesh's particles were already reasonably compact and none fell
below the 10-unit volume floor. The "before vs after" histogram at the bottom shows the same
10-bin split of the *full* volume range (`[30.28, 815.67)`, computed over all 15 particles before
filtering) so the effect of the aspect-ratio removal is visible bin-by-bin — the first bin drops
from 2 particles to 1.

## Notes

- `filter.enabled: false` is useful for a "split only" pass — e.g. when downstream tooling wants
  every particle regardless of shape/size and will do its own filtering later.
- Switching `filter.volume.mode` to `"lognormal_rebalance"` replaces the hard `min`/`max` cutoff
  with a statistical rebalancing pass that thins out overrepresented size bins based on a fitted
  lognormal profile (`filter.volume.bins`/`over_factor` control bin count and how aggressively
  overrepresented bins are thinned) — useful when the goal is a size distribution close to
  lognormal rather than a hard volume floor/ceiling.
- `input.path` also accepts a folder of STL files (each merged and then split), not just a single
  merged STL — convenient when particles were already exported per-file from an earlier stage.
- For the full pipeline mechanics — including how connected components are detected during
  splitting and the exact aspect-ratio/sharpness-ratio definitions — see
  `../reference/pipeline-crop-and-splitfilter.md`.

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] split-filter stage=<name> seconds=<f>` for each completed stage, then `[Timing] split-filter workers=<n>` and `[Timing] split-filter peak_rss_bytes=<n|unavailable>`. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
