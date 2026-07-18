# Packing Target Diameter Distribution

Standard random sequential packing (`PackPipeline::run`) draws candidate particles from whatever
mesh library the user supplied and accepts the first one that clears the geometry filters,
boundary/collision/distance checks, and periodic-ghost checks (mode 3). The *size* of the particles
that end up placed is therefore whatever the input library happens to produce — there is no control
over the resulting equivalent-volume-diameter histogram. The packing-target-distribution feature
adds that control: it steers which bin of a user-supplied diameter histogram each accepted particle
is assigned to, rescaling candidates toward under-represented bins so that, at the end of the run,
the *placed* particles' diameter histogram approximates a target histogram — typically a measured
pore- or particle-size distribution digitized from a real material. The bundled example,
`data/input/gu2019_fig7b_pore_distribution.csv`, is one such digitized curve (Figure 7b from Gu et
al., 2019) with 25 one-unit-wide bins spanning diameters 5–30.

The engine lives in `src/pipeline/pack_targets.rs`; the equivalent-diameter/sphericity metric it
consumes lives in `src/geometry/metrics.rs`; the consumer that wires both into the packing loop is
`PackPipeline::run` in `src/pipeline/pack.rs`. Per `PLAN.md`'s own framing, "accepted-count bin debt"
exists specifically so that "target VF has priority" — the primary objective of packing remains
reaching `target_volume_fraction`, and diameter-distribution matching is a secondary, best-effort
objective layered on top. When a target bin cannot be filled after a bounded number of tries, the
pipeline falls back to whatever bin *can* currently accept a particle, rather than stalling volume
progress in pursuit of a perfect histogram match.

## CSV format and parsing

`load_target_distribution_csv` reads a `bin[,right],frequency` CSV: `bin` is the left edge of a
diameter interval, `right` is its (optional) right edge, and `frequency` is a relative weight. A few
real rows from `gu2019_fig7b_pore_distribution.csv`:

```
bin,right,frequency
5,6,0.12328767123287673
6,7,0.12924359737939248
7,8,0.11673615247170934
```

Parsing rules:

- Columns are restricted to `bin`, `right`, `frequency`; `bin` and `frequency` are required, `right`
  is optional and may be blank per row.
- `bin` values must strictly increase row to row (bins are read in left-edge order).
- A missing `right` is inferred from the **next** row's `bin` value (the two intervals share an
  edge); the **last** row's missing `right` is inferred from the previous bin's width
  (`left + previous_width`). A single-row distribution with no explicit `right` is rejected — there
  is no next row to infer from.
- Explicit `right` values must exceed their own `bin` and must not overlap the previous interval's
  `right`.
- After all rows are parsed, frequencies are normalized so they sum to exactly `1.0`; the raw sum is
  required to already be within `1e-6` of `1.0` before normalization (a sanity check against
  obviously wrong input, not a general renormalization of arbitrary weights).

The resulting `TargetDistribution` holds a `Vec<DiameterBin>` where each bin's interval is
half-open, `[left, right)` — except the very last bin, whose `right` edge is inclusive, so a
diameter exactly equal to the overall maximum still lands somewhere (`bin_for_diameter`). A
diameter falling in a gap between non-adjacent explicit intervals or outside the covered range
returns `None`.

## Metric computation: equivalent diameter and sphericity

Before a candidate mesh can be steered toward any bin, its equivalent-volume diameter must be
computed — and that requires the mesh to be well-formed. `mesh_metrics` first calls the private
`mesh_is_closed`, which validates that every triangle indexes real, distinct vertices, that no face
is duplicated, that every edge is shared by exactly two triangles with opposite winding (manifold,
consistently oriented), and that every edge-connected shell has a nonzero signed volume. Only if
this passes does `mesh_metrics` compute:

- **Volume** and **surface area** via the existing divergence-theorem and triangle-sum routines.
- **Equivalent-volume diameter**: `d = (6V / π)^(1/3)` — the diameter of a sphere with the same
  volume as the mesh.
- **Sphericity**: `Ψ = π^(1/3) · (6V)^(2/3) / A` — the Wadell sphericity, the ratio of a
  volume-equivalent sphere's surface area to the mesh's actual surface area (1.0 for a perfect
  sphere, less for anything else).

If the mesh is open/malformed, or volume/area/diameter/sphericity come out non-finite or
non-positive, `mesh_metrics` returns `None`. In `PackPipeline::run`, a `None` from `mesh_metrics`
on a target-steered candidate simply drops that candidate (`reject_candidate!`) — malformed
candidates are skipped for target-steering purposes rather than causing an error, consistent with
target VF taking priority over any individual candidate.

## Bin allocation algorithm: `TargetDistribution::choose_bin`

This is the core of the feature. For each placement attempt where a target distribution is active,
`choose_bin` decides which histogram bin the next candidate should be pushed toward.

### Bin debt

For every bin not yet attempted in the current placement round, the algorithm computes a **debt**:

```
debt(bin) = frequency(bin) × next_count − observed_count(bin)
```

where `next_count` is the total number of particles accepted so far, plus one (i.e., what the total
would become if this placement succeeds). Debt measures how far a bin's observed share would still
fall short of its target share even after crediting it with the next placement. The bin with the
largest positive debt (strictly greater than a `1e-12` epsilon) is selected — this is the
most-under-represented bin relative to where the running histogram should be heading.

**Verified example** (`tests/pack_target_tests.rs::strict_choice_follows_largest_deficit`): for a
three-bin distribution with frequencies `[0.5, 0.3, 0.2]` starting from an empty state, ten
successive `choose_bin` calls (each immediately recorded as a success) select bins in the order
`[0, 1, 2, 0, 0, 1, 0, 2, 1, 0]`, ending with accepted counts `[5, 3, 2]` — the exact 5:3:2 ratio the
frequencies imply for ten particles. Each of those ten choices is classified `Scaled` (see below),
since a bare `DiameterBin` fixture with no natural-bin hint always requires rescaling.

### Fallback when no bin has positive debt

If every remaining bin's debt is at or below zero (the observed histogram already matches or
slightly over-represents every bin), `choose_bin` switches strategy: for each untried bin, it
computes what the total variation distance (TVD) of the *whole* histogram would be if that bin were
credited with the next placement, and picks the bin that minimizes that projected TVD. Ties within
`1e-15` are broken by the smallest bin midpoint, giving deterministic, reproducible bin choices.

### `BinChoiceKind`: Natural vs. Scaled vs. Fallback

Once a bin index is chosen, `choose_bin` classifies the choice:

- **`Natural`** — the candidate's own, unscaled equivalent diameter already falls inside the chosen
  bin (`natural_bin == Some(index)`), and this is the *first* bin choice attempted for this
  candidate this round (`attempted` was empty when the call started). No rescaling is needed; the
  candidate is used as-is.
- **`Scaled`** — same "first attempt this round" condition, but the candidate's natural diameter is
  not in the chosen bin. `PackPipeline::run` then calls `scale_mesh_to_equivalent_diameter` to
  uniformly rescale the candidate's vertices so its equivalent diameter lands exactly at the chosen
  bin's midpoint (`DiameterBin::midpoint`), and re-verifies (`bin_for_diameter` on the rescaled
  metrics) that the scaled candidate really does land in the intended bin before proceeding.
- **`Fallback`** — the call happens with `attempted` already non-empty, meaning at least one earlier
  bin choice for this candidate/round has already been exhausted. Any bin chosen at this point —
  whether by debt or by the TVD fallback rule — is classified `Fallback` regardless of whether it
  happens to equal the candidate's natural bin, since the algorithm is no longer pursuing its first
  choice.

`attempted` is a caller-owned `BTreeSet<usize>` threaded through repeated `choose_bin` calls within
one placement round; `choose_bin` itself never removes indices from it — only `PackPipeline::run`
does, and only under specific retry conditions (next section).

### Rescaling and re-checking geometry filters

Rescaling a mesh changes its volume, aspect ratio, and surface-to-volume sharpness — all of which
may be gated by `packing.filters`. For that reason `PackPipeline::run` re-applies
`check_geometry_filters` (with volume-filter enforcement turned on) to the **scaled** candidate, not
the original, immediately after scaling. A candidate that passed filters at its natural size can
still be rejected after being scaled up or down to match a target bin.

## Retry and fallback escalation in `PackPipeline::run`

`PackPipeline::run` bounds how hard it will fight for a given bin before giving up on it for this
candidate. A per-round `bin_probe_failures: Vec<usize>` counter (indexed by bin) tracks how many
times a chosen bin has failed *after* being selected (failed scaling, failed geometry filters,
failed boundary/collision/distance/ghost checks, or non-positive in-box volume). The constant
`TARGET_BIN_PROBES = 4` caps this:

- While `bin_probe_failures[index] < TARGET_BIN_PROBES`, a failure for that bin removes it from
  `attempted_bins`, so the *next* `choose_bin` call is free to pick the same bin again (still
  classified `Natural`/`Scaled`, since `attempted` reverts to not having blocked it) — effectively
  giving each bin up to four independent tries with fresh candidates before it counts as
  exhausted.
- Once `bin_probe_failures[index]` reaches `TARGET_BIN_PROBES`, the index is left in
  `attempted_bins` permanently for this round. The next `choose_bin` call then sees a non-empty
  `attempted` set, which is exactly the condition that produces a `Fallback` classification — the
  algorithm has given up steering toward its preferred bin and accepts whatever bin the debt/TVD
  rule finds next.
- If `choose_bin` returns `None` at all (every bin already attempted, none left to try), the
  placement loop for this candidate breaks outright and moves on to the next candidate proposal or
  attempt.
- If a full round of target-bin attempts is exhausted without a successful placement
  (`attempts >= max_attempts` while `had_bin_choice` was true), the whole packing loop terminates
  rather than looping forever chasing an unreachable bin.

## Sphericity steering: `SphericityState` (optional, soft)

Independently of diameter-bin steering, a run can optionally set `target_mean_sphericity` (and an
optional `mean_sphericity_tolerance` band) in the packing config. When set, `PackPipeline::run`
draws up to 4 candidate shapes per placement attempt (`proposal_draws = 4`) instead of one, computes
`mesh_metrics` for each, and scores each with `SphericityState::projected_error`: the distance the
*running mean* sphericity would land outside the `[target − tolerance, target + tolerance]` band if
that particular candidate were accepted next (zero if the projected mean would already be inside the
band). Candidates are sorted ascending by this projected error and tried in that order.

This is explicitly soft, best-effort steering: `projected_error` only ranks candidates relative to
each other within one draw — it never rejects a candidate outright for being far from the target.
`PackPipeline::run` accepts whichever ranked candidate first clears every other check; nothing in the
placement loop refuses a candidate solely because its sphericity is out of range. At the end of the
run, if the final mean sphericity misses the tolerance band, the pipeline prints a warning
("volume fraction was prioritized") rather than treating it as an error.

**Verified example** (`tests/pack_target_tests.rs::sphericity_projection_selects_direction_that_repairs_mean`):
starting from `sum = 0.6, count = 1` (running mean 0.6) with a target of 0.8 and no tolerance, a
candidate with sphericity 0.6 (which would *not* move the mean toward the target) projects to a
larger error than a candidate with sphericity 0.9 (which would move the mean toward 0.75) — the
lower-sphericity candidate's projected error exceeds the higher-sphericity candidate's by exactly
0.05, confirming the ranking correctly favors whichever candidate repairs the running mean.

## State tracking: only successful placements mutate state

`DistributionState` (per-bin `counts` and `attempts`, plus `natural`/`scaled`/`fallback` tallies and
running min/mean/max scale-factor statistics) and `SphericityState` are updated at exactly one point
in `PackPipeline::run`: after a candidate has cleared *every* check — boundary constraints,
pairwise collision, minimum-neighbor distance, periodic ghost collision (mode 3), and a finite,
positive in-box clipped volume. `record_attempt` is called earlier, as soon as a bin is chosen (so
failed attempts still count toward each bin's `attempts` tally for reporting), but `record_success`
and `SphericityState::record_success` are only reached after `current_volume += in_box_volume;
placed.push(candidate);` — i.e., failed placement attempts (any `reject_candidate!` path) never
mutate accepted bin counts, kind tallies, scale-factor statistics, or the sphericity running mean.
This is what makes the debt formula meaningful: `observed_count` always reflects genuinely placed
particles, never near-misses.

## Reporting: `summarize` and `write_distribution_comparison_csv`

After the packing loop ends, if a target distribution was configured, `TargetDistribution::summarize`
computes three headline error statistics from the final `DistributionState`:

- **Total variation distance**: `TVD = 0.5 × Σ |observed_frequency(bin) − target_frequency(bin)|`
  over all bins — the standard TVD between two discrete distributions, bounded in `[0, 1]`.
- **Maximum absolute error**: `max(|observed_frequency(bin) − target_frequency(bin)|)` over bins.
- **Rounding-error baseline** (`rounding_max_absolute_error`): even a hypothetically perfect
  continuous match must still round each bin's ideal fractional share (`frequency × count`) down to
  an integer particle count. `summarize` computes this best-achievable baseline via largest-remainder
  allocation — floor every bin's ideal count, then distribute the leftover particles one at a time to
  the bins with the largest fractional remainders (ties broken by bin index) until the total count is
  exhausted — and reports the maximum absolute frequency error that *this ideal integer allocation*
  would still exhibit. Comparing the actual `max_absolute_error` against this baseline separates
  genuinely unavoidable rounding error (a consequence of packing a finite number of discrete
  particles) from actual algorithmic mismatch. The `strict_choice_follows_largest_deficit` test
  above asserts exactly this relationship: `summary.max_absolute_error <=
  summary.rounding_max_absolute_error + 1e-12`, i.e. the bin-debt algorithm achieves an error no
  worse than the theoretical rounding floor on that scenario.

Separately, `write_distribution_comparison_csv` writes `<output_stem>_diameter_distribution.csv`
next to the packed STL, with one row per bin: `bin, right, target_frequency, target_count,
actual_count, actual_frequency, frequency_error, count_error, attempts`. `PackPipeline::run` also
prints the same per-bin breakdown plus the summary statistics, the natural/scaled/fallback
placement-kind tallies, and (if any scaling occurred) the min/mean/max scale factor applied across
all successfully placed candidates, to stdout as `[Info]` lines. If any placement fell back
(`distribution_state.fallback > 0`), a `[Warning]` line notes that the target distribution was
relaxed to prioritize volume fraction — the same design principle stated in `PLAN.md` surfaced
directly in the run's own output.

## Cross-references

- [pipeline-packing.md](../reference/pipeline-packing.md) — full per-function reference for this
  feature: [`TargetDistribution::choose_bin`](../reference/pipeline-packing.md#targetdistributionchoose_bin),
  [`load_target_distribution_csv`](../reference/pipeline-packing.md#load_target_distribution_csv),
  [`write_distribution_comparison_csv`](../reference/pipeline-packing.md#write_distribution_comparison_csv),
  [`TargetDistribution::summarize`](../reference/pipeline-packing.md#targetdistributionsummarize),
  [`SphericityState::projected_error`](../reference/pipeline-packing.md#sphericitystateprojected_error),
  [`PackPipeline::run`](../reference/pipeline-packing.md#packpipelinerun).
- [geometry-analysis.md](../reference/geometry-analysis.md) — the underlying metric functions:
  [`mesh_metrics`](../reference/geometry-analysis.md#mesh_metrics),
  [`scale_mesh_to_equivalent_diameter`](../reference/geometry-analysis.md#scale_mesh_to_equivalent_diameter).
