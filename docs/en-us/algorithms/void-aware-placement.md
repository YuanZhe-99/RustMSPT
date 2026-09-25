# Void-Aware Placement

The `placement:` engine draws a whole population of particle sizes before it places anything, places
them largest first into the solid region of a domain that may already contain frozen pores, records
every transform, and reports why it stopped. It runs from the `pack` subcommand, selected by the
config carrying a `placement:` block rather than a `packing:` one; the original packing loop is
untouched and is described in
[packing-target-diameter-distribution.md](packing-target-diameter-distribution.md).

The code is `src/pipeline/placement.rs` (the loop), `placement_sizes.rs` (what to place),
`placement_library.rs` (what shapes are available), `placement_feasibility.rs` (whether a proposal is
allowed), `placement_outputs.rs` (what is written), `placement_labels.rs` (the optional voxel field)
and `src/geometry/void_index.rs` (the void).

This document is about the decisions that are not obvious from the code, and the arguments that
justify them. Field-by-field configuration is in
[config.md](../reference/config.md); worked runs with captured output are in
[pack-placement.md](../examples/pack-placement.md) and [pack-void.md](../examples/pack-void.md).

## 1. Determinism

The promise is: same seed, same inputs, same binary, same placement — on any thread count. Four
rules keep it, and breaking any of them fails silently.

**One stream, one thread.** Every variate comes from a single `ChaCha12Rng`, drawn only on the
calling thread. The 32-byte state is `sha256("rustmspt.placement/1|" ++ seed_le)` rather than
`SeedableRng::seed_from_u64`, whose expansion `rand_core` does not promise to keep stable across
major versions. Naming the algorithm and deriving the state here is what makes the promise keepable
across a dependency bump.

**A fixed consumption schedule.** Each attempt draws all of its variates *up front*, before any
check runs: one for the shell, three for the orientation, three for the position (four in
`void_neighbourhood` mode, where the position costs a triangle, two barycentric coordinates and an
offset). Drawing them lazily would make the stream depend on which check short-circuited, so any
later reordering of the checks — and the checks *were* reordered, for speed, after they were first
written — would silently move every placement in every run.

**Speculation rewinds the stream.** Evaluating attempts in parallel does not break the first two
rules, because the draws never leave the calling thread and their count never depends on the outcome:
a batch draws its proposals serially, records the stream position after each (`get_word_pos`),
evaluates them in parallel against the unchanged placed set, and on the first acceptance rewinds to
that attempt's end (`set_word_pos`). Everything drawn after it is discarded and drawn again for the
next particle, so the stream, the tallies and every acceptance are those of a one-at-a-time scan for
any batch size.

**One uniform primitive.** Everything is built from `u01(rng) = (next_u64() >> 11) · 2⁻⁵³`, not
`rand`'s `gen_range` or `Uniform`, which are explicitly not value-stable across minor versions. The
first outputs of the seeded stream are pinned as literals in a test, so a change to either the
algorithm or the derivation fails there rather than in someone's recorded results.

**Nothing order-dependent reaches an output.** The volume accumulator is summed sequentially in
acceptance order, because a parallel `f64` sum depends on the reduction tree and hence on the thread
count — and it is the number that gates the stop. The report emits that same accumulator rather than
re-summing the placed set, so the report cannot disagree with the decision it describes. Rejection
counters live in a `BTreeMap`, sorting is `total_cmp` with the draw index as tie-break, and no
`HashMap` iteration order reaches a file.

That last rule is not hypothetical. `triangulate_cap_from_segments` in `src/geometry/volume.rs` used
`HashSet`s whose `RandomState` is seeded per process *and* bumped per instance, so ring traversal
order — and through it each ring's float centre, and through that the clipped volume's last bits —
differed between two calls in the same process. It is `BTreeSet` now, and a test clips the same mesh
twice and compares bit patterns.

## 2. What to place: the size multiset

**Every size is drawn before any placement begins, and a size that fails is never replaced.**

This is the central design decision, and it exists to prevent one specific failure. An engine that
draws sizes on demand and retries on failure will, in a domain where the large particles no longer
fit, quietly keep drawing until it reaches its volume fraction — with small particles. The result
looks like a successful run at the requested volume fraction, and is the wrong material. Drawing the
whole population first turns that into a shortfall in a named size class, visible in
`size_distribution.csv` and in the report.

Each draw's volume is exact rather than expected: scaling a shell to an equivalent diameter `d` gives
it volume `πd³/6`, by the definition of that diameter.

**The stopping rule is symmetric.** Draws accumulate until the running volume first reaches the
target, then the plan keeps whichever of the last two counts lands *closer*:

```
V_target = basis_volume × volume_fraction
S_k      = Σ_{i≤k} (π/6)·d_i³
K        = min { k : S_K ≥ V_target }
keep K if |S_K − V_target| ≤ |S_{K−1} − V_target|, else keep K−1     (ties resolve downward)
degenerate case K = 0 (the first draw already overshoots): keep 1
```

Stopping at the first count to exceed the target would overshoot every time, and the particle that
overshoots is the largest one drawn — precisely the one most likely to fail and to dominate the
shortfall. `planned_volume_error` is reported signed, so a reader can see how far the plan itself was
off before placement had a chance to make it worse.

**Largest first.** Large particles are the ones that stop fitting, so placing them while there is
still room is what keeps a failure attributable. `placement_order: drawn` restores the drawn order
for anyone who wants it.

**The quantile is Wichura's AS241**, accurate to about `1e-15`, not the Abramowitz & Stegun
approximation in `split_filter.rs`, which is good to `1.5e-7`. A truncated lognormal's bounds are
evaluated in the tails, where that error visibly moves how much mass is cut off and therefore how
many large particles a run plans for. The coefficients are stored highest-degree-first, as the paper
prints them; the first Horner loop written for them assumed ascending order, returned plausible but
wrong quantiles, and was caught only because the test compares against a published quantile table
rather than against this crate's own CDF.

## 3. What may be placed: the shape library

Files are listed explicitly and split into closed shells in first-face order, so `(source_index,
shell_index)` is a reproducible address. Directory listing order is never used: it is OS-dependent.

**An open or inward-wound shell refuses the run.** Its volume, centroid and equivalent diameter are
what the size distribution and the record are built from, so skipping one would change what the run
drew from without saying so. A shell that fails a *filter* is different: aspect and sharpness ratios
are scale-invariant, so such a shell fails at every size, and it is recorded as rejected with the
ratio that failed.

**A shell ordinal is not an identity.** Re-exporting a file with its faces permuted moves every
ordinal silently, so each shell also carries `shell_sha256`, a digest of its own geometry. A
consumer can then tell "the same shape at a different ordinal" from "a different shape at the same
ordinal".

Each shell is stored translated so its **volume** centroid sits at the origin. That makes the
placement transform a plain scale-rotate-translate with no pivot bookkeeping left, and it is why the
record's `shell_centroid` is the volume centroid and not `mesh_centroid`, which is a vertex mean and
moves when a region is tessellated more finely.

The library also reports `max_extent_ratio`: the largest ratio of a shell's bounding radius to half
its equivalent diameter. Scaling preserves shape, so a value well above 1 says a particle "of
diameter d" reaches much further than d/2 from its centre — which is what actually decides whether
it fits a channel. On the repository's own `particles.stl` it is 1.84.

## 4. Orientation

`uniform_so3` is Shoemake's method (Ken Shoemake, *Uniform Random Rotations*, Graphics Gems III,
1992): from three uniforms `u₁, u₂, u₃`,

```
q = ( √u₁·cos 2πu₃,  √(1−u₁)·sin 2πu₂,  √(1−u₁)·cos 2πu₂,  √u₁·sin 2πu₃ )   [w, x, y, z]
```

is uniform on the unit 3-sphere, hence Haar-uniform on SO(3) through the double cover. Exactly three
draws, always.

This is **not** what the original engine's `rotation_mode: any` does. That samples an axis uniformly
inside a cube and an angle uniformly on `[0, 2π)`, which is not uniform on rotations. It keeps its
old name and its old behaviour; the two are documented as different.

Sampled quaternions are canonicalised to `w ≥ 0`. `q` and `−q` are the same rotation, so without
that convention a recorded quaternion would not be a function of the seed. The test that checks
uniformity does so against Haar's *own* angle density `(1 − cos θ)/π` rather than against flatness —
a flat angle histogram is exactly what the axis-and-angle sampler produces, so testing for flatness
would have confirmed the wrong thing.

## 5. Position, and what "uniform" claims

Stated exactly, and repeated in the report as `samplers.position`:

> Given the drawn size, the drawn shell and the particles already accepted, the placement is uniform
> on the set of feasible (centroid, orientation) pairs with respect to Lebesgue × Haar measure: the
> centroid is proposed uniformly in a box that contains every feasible centroid, the orientation is
> Haar-uniform, and only feasible proposals are accepted. The assembly as a whole is a **random
> sequential adsorption** configuration, not an equilibrium hard-core one.

Three qualifications matter.

**The proposal box must not depend on the orientation.** In `strict` mode the domain is eroded by the
particle's circumscribed-sphere radius — orientation-independent by construction. Eroding by the
*rotated* bounding box instead, which is the obvious thing to write, makes the proposal density
`∝ 1/|box(q)|` and under-weights orientations with a smaller footprint, correlating orientation with
position near the walls.

**RSA is not equilibrium.** Because the feasible set depends on the particles already accepted, the
joint law is sequential adsorption, whose pair correlations differ measurably from an equilibrium
hard-core packing. The report says `rejection_uniform_rsa` rather than anything shorter.

**This is not uniform coverage of the solid.** Uniformity is over feasible placements, not over the
solid phase, and `void_neighbourhood` is not uniform at all — it is a deliberate construction,
reported as `void_neighbourhood_band`, and never as anything containing the word "uniform".

## 6. The void

### 6.1 The predicate, and why it is complete

With `crossing: forbidden`, a placement is allowed when all three hold:

- **(a)** no particle vertex lies inside the void,
- **(b)** the surfaces do not intersect, and their minimum distance is at least `g_pv`,
- **(c)** no void vertex lies inside the particle.

*Claim.* For closed, non-self-intersecting surfaces, (a) ∧ (b) ∧ (c) is equivalent to: the particle
solid and the void solid are disjoint and separated by at least `g_pv`.

*Argument.* By (b) the two surfaces never cross, so each closed solid is either wholly inside or
wholly outside the other. (a) rules out the particle lying inside the void. (c) rules out the void
lying inside the particle. (b) supplies the separation in the remaining, disjoint case. A closed
shell strictly inside another has *all* of its vertices inside it, so neither vertex test can miss
the case it exists for.

(c) is the one people leave out, and without it a particle large enough to swallow a whole pore
passes every other test.

**The same predicate governs a pair of particles**, and v0.2.0 did not apply it there. Between two
particles there is no "which one is the void" asymmetry, so (a) and (c) collapse into one symmetric
question — does either solid contain the other — answered by `mesh_solids_nested_prepared`, while
(b) is `mesh_surfaces_intersect_prepared` plus the gap test. Leaving it out is not a subtle loss:
on a lognormal from 3 µm to 45 µm at a 0.30 target, v0.2.0 placed 146 particles of which **28 lay
wholly inside another** — one of them three deep — double-counting 2.1 % of the domain and
reporting `target_reached` on an inflated fraction. The shipped examples never showed it because
their 6-to-24 range makes the nesting region about 500 µm³ in a 10⁶ µm³ domain. A rejection is
reported as `particle_enclosed`.

`g_pv > 0` is required, not merely recommended: `parry`'s `query::distance` returns exactly `0.0` for
two shapes that intersect, so `0.0 >= 0.0` would pass and a zero gap would forbid nothing. The config
refuses it by name.

### 6.2 Cost

A box prefilter bounds everything. If no void triangle comes within `g_pv` of the particle's bounding
box, the particle is both farther than `g_pv` from every void triangle **and** not nested inside a
shell — because a nested particle's box necessarily intersects that shell's own hierarchy leaves.
One `Qbvh::intersect_aabb` call therefore skips all three tests for the overwhelming majority of
proposals.

### 6.3 Inside-the-void is ray parity

`VoidIndex::contains_point` casts a fixed, non-axis-aligned ray over the hierarchy and counts
crossings, deduplicating hits within `1e-8`. Not a pseudo-normal test: parity is correct for a shell
nested inside another — a pore inside a pore is a real case — and does not care which way the faces
wind. It uses the same ray direction and tolerance as `s2::point_inside_mesh`, and a test asserts the
two agree on thousands of points, because two implementations that disagree give no way to tell from
inside either which one is wrong.

### 6.4 Orientation, and why a mixed void is refused

`mesh_volume` returns the absolute value of the whole divergence sum. A void of two pores with one
inverted would therefore report `|V₁ − V₂|` instead of `V₁ + V₂`, and a solid basis computed against
it would be quietly wrong. Shells that disagree are refused at load time with a message that says
why. An all-inward void is fine — parity does not care — and is reported as `inward`.

### 6.5 The solid basis

`target.basis: solid` means the domain minus the void. When the void lies inside the domain its
shells' volumes add exactly (`exact_shell_sum`); otherwise the part inside is clipped exactly
(`exact_clip`). The method is reported, never assumed, because the basis changes what the target
volume fraction *means*.

### 6.6 Crossing allowed

The centre of a particle may never sit inside a pore — such a particle is not in the solid phase at
all — but the rest of it may reach in. The overlap is counted on voxels anchored to the **domain**
origin, never to each particle's own box: two particles reaching into the same pore must agree about
the same physical voxel, or their overlaps double-count where they meet.

The void owns that volume. The record publishes the identity rather than leaving a reader to infer
it:

```
volume.in_domain_solid = volume.in_domain − void_overlap_volume_in_domain
```

with `in_domain` gross and `in_domain_solid` the number that sums to the solid phase. `voxel_size`
has no default: it is the same knob for cost and for accuracy, and the trade is the caller's.

## 7. The check order

One fixed order, cheapest and most selective first, each arm returning its own named reason — which
is what makes the report's rejection tally readable as a funnel:

1. `neighbourhood_band` (that mode only)
2. `outside_domain` (strict) or `boundary_depth` (clip)
3. `inside_void`
4. `void_gap`
5. `void_enclosed`
6. `particle_overlap`
7. `particle_enclosed`
8. `particle_gap`
9. `zero_in_domain_volume`

Three expensive things are deliberately **deferred** behind cheap rejections, and it matters by more
than an order of magnitude: writing them eagerly made an 86-particle run take 3.44 s, and deferring
them took the same run, placing the same particles, to 0.21 s.

- The `parry` `TriMesh` is built only when a neighbour survives a centre-to-centre sphere test and a
  box test. Building one per attempt means building a bounding-volume hierarchy per attempt.
- The exact in-box volume runs only for a particle whose box actually straddles the domain. One
  wholly inside keeps its full volume by definition.
- The exact pair distance is reached only after both cheaper tests fail to separate the pair, and
  even then only for a pair a bounded screen at the gap cannot show to be farther apart
  (`mesh_closer_than_prepared`); the answer is the same as measuring every such distance.

A particle's full volume is `scale³ × shell.volume`, never a fresh tetrahedra sum over the
transformed mesh.

## 8. Volumes in the domain

The engine does not use `clip_mesh_by_bbox` for in-domain volume. `mesh_volume_in_bbox_exact` applies
the six planes one at a time, and after each one closes the hole that plane opened by fanning every
edge the clip created back to a single fixed apex on that plane.

A fan reproduces a closed loop's signed area exactly, whatever shape the loop has — the overlapping
pieces cancel — so nothing assumes a cap is star-shaped, and a loop nested inside another arrives
with the opposite winding and subtracts itself. That is precisely what the legacy
`triangulate_cap_from_segments` gets wrong: it re-sorts each ring by angle about its own vertex mean,
which is only valid for a star-shaped ring, and it forces both rings of an annulus to the same
winding, so a hole is filled rather than subtracted.

Capping *before* moving to the next plane is also what supplies each later cap with its corner edges.
The segment where two domain faces meet bounds both caps but lies on neither's surface, so a routine
that clipped all six planes first would leave every multi-plane cap loop open. That was measured: on
a `[0,2]³` cube, one plane gave the exact 4.0, two planes gave 1.333 against a true 2.0, and three
gave ~0 against a true 1.0.

## 9. Stopping

Evaluated top to bottom; the first match wins.

| # | Condition | `stop_reason` |
|---|---|---|
| 1 | nothing was placed | `no_feasible_placement` |
| 2 | any planned size failed (`shortfall > 0`) | `distribution_unattainable` |
| 3 | the whole-run attempt budget is spent | `budget_exhausted` |
| 4 | everything planned was placed and the target was reached | `target_reached` |
| 5 | everything placed, but clipping left the fraction short | `target_reached`, with a warning |

Two of the orderings carry the weight.

**Placed-nothing outranks everything**: a run with no particles has no distribution to describe.

**Unattainable outranks budget-exhausted**, and this is the one that is easy to get backwards.
Exhausting the per-particle budget is *how* an unattainable size is detected. Ordered the other way,
every distribution failure would report as a budget failure, and the requirement that the run say why
it stopped would never be met.

The vocabulary is fixed at four words. A run that placed everything it planned but lost volume at the
domain boundary is `target_reached` with the deficit named in `stop_detail`, not a fifth word — a
consumer's adapter is written against this list, and a fifth value would reach it as an unknown one.
`stop_detail` is always an object and always carries a `message` suitable for a log line.

One combination is worth knowing: `on_unattainable: stop` with the default `descending` order ends
with **zero** particles if the largest planned size cannot fit, and the reason is then
`no_feasible_placement` by rule 1, not `distribution_unattainable`.

## 10. Top-up

A top-up batch is drawn only when **every** planned size was placed and the solid volume is still
below `target × (1 − tolerance)` — that is, when the deficit is arithmetic or from boundary clipping,
never from a failure. After a failure, a batch would be drawn from the same distribution, would
mostly produce small particles, and would quietly make up the missing volume: the one thing the
draw-everything-first design exists to prevent.

Top-up counts are tallied in their own CSV columns, never merged into the target ones, so the file
cannot hide a shortfall behind them. In `strict` mode nothing is clipped, so the path is reachable
only through the plan's own rounding.

## 11. Cross-references

- [config.md](../reference/config.md) — the `placement:` block, every field and every refusal.
- [pipeline-placement.md](../reference/pipeline-placement.md) — per-function reference.
- [pack-placement.md](../examples/pack-placement.md), [pack-void.md](../examples/pack-void.md) —
  worked runs with captured output.
- [mesh-clipping-volume-fraction.md](mesh-clipping-volume-fraction.md) — the legacy clipper, its
  star-shaped-cap limitation and its nested-ring behaviour.
- [packing-target-diameter-distribution.md](packing-target-diameter-distribution.md) — the original
  engine's histogram steering, which is a different design for a different problem.

### CPU worker budget (PERF-02)

Each `run_placement` call installs the entire run in a dedicated Rayon pool. This includes
voxel label generation and any nested parallel geometry calls. The report reads the active worker
count inside that pool. RNG consumption, acceptance order and floating-point accumulation remain
sequential. Regression tests compare records, STL, CSV, phase/particle-id TIFFs and voxel headers
byte for byte at 1, 2 and 8 workers.

Label generation now uses 1024-voxel tiles, sorted spatial candidates and cached per-particle parity queries. It preserves void precedence and first-particle ownership; full phase/id arrays are still resident.
