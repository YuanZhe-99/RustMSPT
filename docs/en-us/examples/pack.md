# `pack` pipeline example

## What it does

`pack` performs random-sequential packing of candidate particle meshes into a rectangular box: it
repeatedly proposes a candidate particle (optionally rotated), tests it against geometry filters,
domain-boundary rules, pairwise collision, minimum-neighbor-distance, and (in periodic mode)
ghost-image collision, and accepts the first candidate per attempt that clears every check — until
either `target_volume_fraction` is reached or `max_attempts` is exhausted. This example runs the
**plain** mode: no target diameter-distribution steering, just "fill the box to X% solid volume
fraction with whatever candidate shapes come up." For the distribution-steering variant, see
[`pack-target-distribution.md`](pack-target-distribution.md).

## Config

The repository's real default config, `data/input/pack_config.yaml`, is a target-distribution
config (it sets `target_diameter_distribution_csv`). For this plain-packing walkthrough that field
is commented out and the output path is redirected to a scratch location; everything else —
including `target_volume_fraction: 0.02`, the real default — is unchanged:

```yaml
# Input settings
input:
  # Path to a single STL or a folder of STLs
  path: "data/input/particles.stl"

# Output settings
output:
  path: "/tmp/rustmspt-doc-examples/pack_plain_result.stl"

# Box dimensions (The container)
box:
  dimensions: [100.0, 100.0, 100.0] # X, Y, Z size

# Packing algorithm settings
packing:
  # Target Volume Fraction (0.0 to 1.0). E.g., 0.3 means 30% filled.
  target_volume_fraction: 0.02

  # Mode 1: Strict (No boundary crossing)
  # Mode 2: Loose (Boundary crossing allowed, ignore walls)
  # Mode 3: Periodic (Boundary crossing allowed + Periodic collision checks)
  mode: 2

  # Max attempts to place a particle before giving up (prevents infinite loops)
  max_attempts: 2000

  # Constraint: Minimum distance between particles (0.0 to allow touching)
  min_neighbor_distance: 1.0

  # Rotation constraint for candidate particle placement
  # Options: 'none', 'x', 'y', 'z', 'vector', 'any'
  # 'vector' uses rotation_axis_vector as axis direction and rotates around particle centroid.
  rotation_mode: 'none'
  rotation_axis_vector: [0.0, 0.0, 1.0]

  # d_1: Distance for fully internal particles
  min_boundary_dist: 1.5
  # d_2: Minimum protrusion/retention depth for crossing particles
  min_cross_boundary_depth: 3.0

  # CPU worker limit for packing collision/distance checks
  # -1 means no limit (use all available cores)
  cpu_max: -1

  # Orientation fix before final output (can be expensive)
  # false by default to skip; set true to enforce positive signed volume orientation.
  orient_to_positive_volume: false

  # target_diameter_distribution_csv: "data/input/gu2019_fig7b_pore_distribution.csv"

  # Geometry filters.
  filters:
    min_volume: 15.0
    max_aspect_ratio: 3.0    # Longest side / Shortest side
    max_sharpness_ratio: 2.0  # (Area^3) / (36*pi*Volume^2). Sphere=1.0. Higher = Sharper/Irregular.
```

`input.path` points at `data/input/particles.stl`, a single STL that acts as a small library of
candidate particle shapes: `PackPipeline` pre-loads it and reports how many separate solid shells
(candidate particles) it contains before packing starts (`14` in this run — see stdout below).
Each placement attempt draws one of those 14 candidates, applies the configured `rotation_mode`,
and tries to place it.

| Field | Meaning |
|---|---|
| `target_volume_fraction` | Solid volume fraction the pack should reach (`0.02` = 2% filled here — deliberately small so the walkthrough finishes in well under a second; production runs typically target much higher fractions and take proportionally longer). |
| `mode` | `1` strict (no boundary crossing), `2` loose (crossing allowed, walls ignored), `3` periodic (crossing allowed + periodic ghost-collision checks). This example uses `2`. |
| `max_attempts` | Placement attempts allowed before the pipeline gives up and reports whatever volume fraction it reached. |
| `min_neighbor_distance` | Minimum allowed gap between placed particle surfaces. |
| `rotation_mode` / `rotation_axis_vector` | Controls candidate orientation before placement; `'none'` here means every candidate keeps its as-loaded orientation. |
| `min_boundary_dist` / `min_cross_boundary_depth` | Boundary-crossing tolerances (`d_1`/`d_2`) used in loose/periodic modes. |
| `cpu_max` | Worker-thread cap for collision/distance checks; `-1` uses every available core. |
| `orient_to_positive_volume` | Optional post-pack orientation-fix pass (expensive; off by default). |
| `filters` | Per-candidate geometry gates (minimum volume, maximum aspect ratio, maximum sharpness ratio) applied before a candidate is even attempted for placement. |

See [`../reference/config.md`](../reference/config.md) (`packing.rs` section: `PackingFilters`,
`PackingParams`, `PackingConfig`) for the full field reference, including the target-distribution
fields (`target_diameter_distribution_csv`, `target_mean_sphericity`,
`mean_sphericity_tolerance`) left unset/commented in this plain example.

## Running it

```bash
./target/release/rustmspt pack --config /tmp/rustmspt-doc-examples/pack_plain.yaml
```

(`/tmp/rustmspt-doc-examples/pack_plain.yaml` is the scratch copy of `data/input/pack_config.yaml`
shown above; the repository's own `data/input/pack_config.yaml` is left untouched.)

## Expected output

Real captured stdout from the run above:

```
[Info] Packing input mode: preloaded single STL (14 candidate particles)
[Info] CPU setting: cpu_max=-1 -> using 8 worker threads (available 8).
[Info] Rayon pool threads (effective): 8
[Info] Rotation mode: none
[Info] Packing completed.
[Info] Final count: 63
[Info] Final volume fraction: 0.020161
[Info] Orientation fix enabled: false
```

The run completed in `real 0m0.290s` and placed 63 particles, landing at a final volume fraction
of `0.020161` — just over the `0.02` target, since the pipeline stops as soon as a placement pushes
the running volume fraction at or past the target rather than trying to land exactly on it.

> **These numbers are one run's.** The `packing:` engine takes no seed, so the count varies between
> runs of this same config — measured at 52, 60, 60 and 62 on four consecutive runs. Only the volume
> fraction is pinned, and only from below, by the stopping rule. For a run that reproduces exactly,
> use the `placement:` engine ([pack-placement.md](pack-placement.md)), which takes a seed and
> records what it did. Since 0.2.1 this loop also rejects a candidate that would sit wholly inside a
> placed particle; see [spatial-grid-collision.md](../algorithms/spatial-grid-collision.md#why-nesting-needs-its-own-test).

The output STL, `/tmp/rustmspt-doc-examples/pack_plain_result.stl`, is a real binary STL file
(1,195,884 bytes on disk) containing 23,916 triangles — the union of all 63 placed particle
instances, each a rotated/translated copy of one of the 14 candidate shells from
`data/input/particles.stl`. There is no companion CSV report in this mode: the
`<output_stem>_diameter_distribution.csv` report is only produced when
`target_diameter_distribution_csv` is set (see the target-distribution example).

## Notes

- Because no `target_diameter_distribution_csv` is set, candidates are used at their natural size —
  there is no diameter-bin steering or rescaling pass, and the placed-particle size distribution is
  purely whatever `data/input/particles.stl`'s 14 candidate shapes happen to produce under random
  selection and rotation.
- `filters.min_volume` (`15.0` here) still applies in plain mode: candidates smaller than this are
  rejected before any placement geometry check runs, regardless of the target volume fraction.
- Increasing `target_volume_fraction` increases both attempt count and wall-clock time
  super-linearly as the box fills up and collision-free space becomes scarce; `0.02` was chosen here
  specifically to keep this walkthrough fast (well under a second). See
  [`../reference/pipeline-packing.md`](../reference/pipeline-packing.md) (`PackPipeline::run`) for
  the exact per-attempt check sequence (filters -> boundary -> collision -> neighbor distance ->
  periodic ghost checks in mode 3).
- For the same input library and box, steering the placed-particle *sizes* toward a measured
  diameter distribution instead of accepting them as-is is covered in
  [`pack-target-distribution.md`](pack-target-distribution.md) and
  [`../algorithms/packing-target-diameter-distribution.md`](../algorithms/packing-target-diameter-distribution.md).

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] pack stage=<name> seconds=<f>` for each completed stage, then `[Timing] pack workers=<n>` and `[Timing] pack peak_rss_bytes=<n|unavailable>` and one or two `[GridStats]` lines. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
