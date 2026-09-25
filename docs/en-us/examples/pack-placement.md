# `pack` with a `placement:` block — seeded, recorded placement

## What it does

`pack` selects an engine from the config it is given. A top-level `placement:` block runs the
seeded, recorded, void-aware engine; a `packing:` block runs the original packing loop, unchanged.
Exactly one must be present, and a config with both or neither is refused by name.

This walkthrough is the plain case: an empty domain, no void. For packing around a frozen void see
[`pack-void.md`](pack-void.md); for the original engine see [`pack.md`](pack.md).

Three things distinguish this engine from the original one, and they are what the rest of this page
is about:

- **The run is reproducible.** The same seed, inputs and binary place the same particles, on any
  thread count.
- **Every particle is recorded.** A per-particle JSON file carries the source shell, the scale, the
  rotation and the translation, so any particle can be rebuilt from the record alone.
- **The run says why it stopped**, from a fixed four-word vocabulary, in a JSON report beside the
  geometry.

## Config

The repository's committed config, `data/input/placement_config.yaml`, verbatim:

```yaml
# Seeded, recorded placement into an empty domain.
#
# A `pack` config carries exactly one of `placement:` or `packing:`, and which
# one decides the engine. This selects the seeded, recorded, void-aware engine.
# For the original packing loop see pack_config.yaml, which is unchanged.
#
# Every relative path below resolves against the directory holding THIS file
# (data/input/), never against the working directory.
placement:
  # Required. The same seed, inputs and binary give the same placement, on any
  # thread count. Overridable with --seed.
  seed: 20260910

  # A label, copied into every output. Nothing scales by it.
  frame:
    unit: "um"

  domain:
    min: [0, 0, 0]
    max: [100, 100, 100]

  shapes:
    # Each file is split into closed shells in first-face order. List order
    # fixes the source index, so (source, shell) is a reproducible address.
    files: ["particles.stl"]
    # Scale-invariant, so they are applied once to the library rather than to
    # every rescaled candidate.
    filters:
      max_aspect_ratio: 3.0

  size:
    # A truncated lognormal in diameter. The alternative is
    # { kind: histogram, csv: "..." }, which reads the same CSV format the
    # original engine uses.
    distribution:
      kind: lognormal
      median: 12.0
      sigma_log: 0.35
      min: 6.0
      max: 24.0
    # Reporting classes for the target-against-actual table.
    classes: { kind: equal_width, count: 6 }

  # Shoemake's uniform unit quaternion: Haar-uniform on SO(3). The original
  # engine's rotation_mode: any is an axis-and-angle sampler and is not.
  orientation: { mode: uniform_so3 }

  # Clearance between placed particles.
  gaps: { particle_particle: 1.0 }

  target:
    volume_fraction: 0.10
    # basis defaults to `domain` without a void, `solid` with one.

  budget:
    attempts_per_particle: 3000
    total_attempts: 2000000

  outputs:
    dir: "../output/placement"
```

See [`../reference/config.md`](../reference/config.md) (`placement.rs` section) for every field,
every default, and every refusal with the reason it exists.

### The path contract, in one sentence each

*Relative paths in a config resolve against the directory containing that config file; the current
working directory is never consulted.* That is why `files: ["particles.stl"]` finds
`data/input/particles.stl` and `dir: "../output/placement"` writes to `data/output/placement`,
whatever directory the binary was launched from.

*Paths given on the command line resolve against the current working directory, as shell arguments
do.* `--input` and `--output` are made absolute before they are substituted into the block, so both
rules hold at once.

A placement config found through the legacy working-directory default (`data/input/pack_config.yaml`
when `--config` is omitted) is **refused**. The engine's promise is that nothing it reads depends on
where it was launched from, and honouring a working-directory default would quietly break it.

## Running it

```bash
./target/release/rustmspt pack --config data/input/placement_config.yaml
```

## Expected output

Real captured stdout from the run above:

```
[Info] Placement seed 20260910 (chacha12), -1 thread setting
[Info] Placed 82 particle(s); volume fraction 0.099372 of the domain basis (target 0.100000)
[Info] Attempts 185 of a 2000000 budget
[Info] Stop reason: target_reached
[Info] every planned size was placed and the target volume fraction was reached
[Info] Wrote data/input/../output/placement
```

The run took `real 0m0.351s` and wrote four files into `data/output/placement/`:

| File | Bytes | What it is |
|---|---|---|
| `particles.stl` | 1,709,484 | The merged particle geometry, in acceptance order |
| `particles.json` | 163,172 | One record per particle, with its transform |
| `run_report.json` | 5,430 | Identity, seed, inputs, targets, actuals, rejections, stop reason |
| `size_distribution.csv` | 467 | Target against actual, per size class |

`data/input/../output/placement` is the resolved path exactly as the config wrote it. It is not
normalised, because `a/../b` is only the same as `b` when `a` is not a symbolic link, and the engine
resolves paths lexically rather than touching the filesystem.

## The size distribution, target against actual

Real captured contents of `data/output/placement/size_distribution.csv`:

```
class,lo,hi,target_frequency,target_count,drawn,placed,shortfall,top_up_drawn,top_up_placed
0,6.000000000000,9.000000000000,0.190818581175,16,23,23,0,0,0
1,9.000000000000,12.000000000000,0.309181434576,25,22,22,0,0,0
2,12.000000000000,15.000000000000,0.250033318666,21,19,19,0,0,0
3,15.000000000000,18.000000000000,0.145479545122,12,11,11,0,0,0
4,18.000000000000,21.000000000000,0.071838178127,6,5,5,0,0,0
5,21.000000000000,24.000000000000,0.032648942335,3,2,2,0,0,0
```

`drawn` equals `placed` in every class and `shortfall` is zero throughout: everything the run
planned, it placed.

The columns are worth reading carefully, because the difference between them is the point.

- `target_count` is what the class's share of the distribution implies for this many particles.
- `drawn` is what the run actually drew. It differs from `target_count` by sampling noise, which at
  82 particles is what you would expect.
- `placed` is what fitted. **`drawn` minus `placed` is `shortfall`, and nothing is ever drawn to
  make it up.** Every size is drawn before any placement begins, and a size that fails is never
  replaced. That is what stops a run from quietly reaching its volume fraction by adding small
  particles when the large ones would not fit — the failure stays visible, in the class it belongs
  to, instead of disappearing into the total.
- `top_up_drawn` and `top_up_placed` stay in their own columns for the same reason. A top-up batch
  is only ever drawn when **every** planned size was placed and volume was lost at the domain
  boundary; after a failure it is never drawn at all.

## The per-particle record

The header of `data/output/placement/particles.json`, captured:

```json
{
  "schema_version": "rustmspt.placement.record/1",
  "tool": {
    "name": "rustmspt",
    "version": "0.2.1",
    "git_commit": "b63b1a308208efcc39069c2a16d16057900a5219",
    "git_dirty": false,
    "features": [
      "default"
    ],
    "target": "aarch64-unknown-linux-gnu",
    "host": "aarch64-unknown-linux-gnu",
    "profile": "release"
  },
  "seed": 20260910,
  "rng": "chacha12",
  "frame": {
    "unit": "um",
    "origin": [0.0, 0.0, 0.0],
    "axis_order": "xyz",
    "handedness": "right",
    "domain": {
      "min": [0.0, 0.0, 0.0],
      "max": [100.0, 100.0, 100.0]
    }
  },
  "conventions": {
    "rotation": "unit quaternion, component order w,x,y,z (scalar first), canonicalised to w >= 0; `matrix` is derived from it and must agree",
    "shell_centroid": "the volume centroid of the closed source shell, by the signed-tetrahedron formula. Use the value recorded here rather than recomputing one: a vertex mean is a different point.",
    "stl_precision": "the merged STL stores float32 with zero normals; this record stores float64. Reconstruction agrees to within 1.000e-3 in this frame, which is over a hundred times the float32 storage error and far below any transform mistake.",
    "transform": "p_world = R(q) * (scale * (p_source - shell_centroid)) + translation",
    "translation": "the placed particle's volume centroid, in the run frame, in um",
    "triangle_range": "half-open [start, end) into the merged STL's triangle list",
    "volume": "in_domain_solid = in_domain - the part of the particle the void owns; in_domain is gross"
  },
```

The `conventions` block is not documentation that happens to live in the file. It is the contract,
put where a consumer will read it: every one of those sentences is something a reader could get
wrong in a way that produces a plausible, wrong particle rather than an error. The component order
of a quaternion is the clearest case — `nalgebra` stores its quaternions `[i, j, k, w]` while this
record publishes `[w, x, y, z]`, and reading one as the other gives a perfectly valid rotation that
is not the one that was used.

The first particle, captured in full:

```json
{
  "entity_id": "p000000",
  "acceptance_index": 0,
  "source_shape": {
    "source_index": 0,
    "path": "particles.stl",
    "sha256": "77fc24b9144a6541d604777ff697bc1fed24d2ad5d3a86239e2b068e508bc2f1",
    "shell_index": 1,
    "shell_sha256": "aeb8557dac883c95655a8111586e95662a7d63539faa6a34dbf2d8e33ed60d07",
    "shell_centroid": [166.78509225061003, 632.7816179920254, 1216.7894042395735],
    "shell_volume": 291.5236739545008,
    "shell_equivalent_diameter": 8.226688793732556
  },
  "scale": 2.8713896902584612,
  "rotation": {
    "quaternion": [0.3678065941127522, 0.6762105784147148, 0.0008832030275986394, -0.6383234156128273],
    "matrix": [
      [0.1850848740655714, 0.4707535853382458, -0.8626323963794305],
      [-0.4683646604176454, -0.7294350585591789, -0.4985569578459734],
      [-0.8639317879693951, 0.49630188115294804, 0.08547694718489784]
    ]
  },
  "translation": [21.76732699091492, 52.03848224625089, 45.96408596707188],
  "equivalent_diameter": 23.622029387288478,
  "volume": {
    "full": 6901.607209544339,
    "in_domain": 6901.607209544339,
    "in_domain_solid": 6901.607209544339
  },
  "clipped": { "any": false, "faces": [] },
  "void_overlap_volume": 0.0,
  "size_class": 5,
  "triangle_range": [0, 402],
  "bbox": {
    "min": [9.026609545024524, 32.94119717830499, 33.37545645953148],
    "max": [33.05700803312416, 70.13967774209155, 58.39314874515995]
  }
}
```

Two fields repay attention.

`shell_sha256` sits beside `shell_index` because an ordinal says how a shape was *found*, not which
shape it *is*. Re-exporting `particles.stl` with its faces in another order moves every ordinal
silently; the digest is over the shell's own geometry, so a consumer can tell the two situations
apart.

`triangle_range` is `[0, 402)` into `particles.stl`'s triangle list, so this particle's geometry can
be pulled straight out of the merged file without reconstructing anything. The ranges tile the file
exactly, with no gap and no overlap.

The largest particle is placed first: `size_class: 5` is the top class and `acceptance_index: 0`.
Sizes are placed largest first by default, because large particles are the ones that stop fitting,
and placing them while there is still room is what keeps a failure visible as a shortfall rather
than as a run that quietly became a pile of small ones.

## Reproducibility, checked

The two runs below differ only in their thread count:

```bash
./target/release/rustmspt pack --config data/input/placement_config.yaml --output /tmp/t1 --threads 1
./target/release/rustmspt pack --config data/input/placement_config.yaml --output /tmp/t8 --threads 8
sha256sum /tmp/t1/particles.json /tmp/t8/particles.json
```

Both digests are the same. So are the two `particles.stl` files, byte for byte. A different `--seed`
gives a different assembly.

`--seed` and `--threads` apply to this engine only. Passing either with a `packing:` config is an
**error**, not a no-op: accepting a determinism flag and ignoring it would report a run as
reproducible when it is not.

## Notes

- The engine's `orientation.mode: uniform_so3` is Shoemake's uniform unit quaternion, which is
  Haar-uniform on SO(3). The original engine's `rotation_mode: any` samples an axis inside a cube
  and an angle uniformly, which is not; it keeps its old name and its old behaviour.
- Position sampling is reported as `rejection_uniform_rsa`. Given the drawn size, the drawn shell
  and the particles already accepted, a placement is uniform on the set of placements that satisfy
  every constraint — but the assembly as a whole is a random sequential adsorption configuration,
  not an equilibrium hard-core one, and the name says so rather than claiming more.
- A run that stops short of its target still **exits zero** and says so in the report. Only an
  unusable config or an output that cannot be written is an error.
- For the full algorithm, including the completeness argument for the void predicate and the
  stop-reason precedence, see
  [`../algorithms/void-aware-placement.md`](../algorithms/void-aware-placement.md).

### Timing lines (added 2026-09-25)

Captured output above predates the shared stage timer. Current builds also print `[Timing] placement stage=<name> seconds=<f>` for each completed stage, then `[Timing] placement workers=<n>` and `[Timing] placement peak_rss_bytes=<n|unavailable>`. Stage names are listed in `../reference/pipeline-core.md` (`pipeline/timing.rs`); output files are unchanged.
