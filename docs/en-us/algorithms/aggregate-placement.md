# Deterministic aggregates and proxy placement

This opt-in path belongs to `pack` / `placement:`. An omitted `aggregates` block,
or `enabled: false`, leaves the original particle-by-particle path unchanged.
This implementation is not used by legacy `packing:` configs.

## Two separate stages

1. Build a configurable catalog of `variants` templates, each with
   `particles_per_cluster` real source particles. Template generation never draws
   random positions, random sizes or random retry proposals.
2. Plan repeated whole templates by their **summed member material volume** and
   place those rigid clusters with seeded global RSA. Only simple conservative
   container proxies participate in global collision rejection. Accepted clusters
   are expanded into ordinary individual particle records and STL geometry.

The internal constructor is deterministic **FCC + swept-contact initialization + mesh coordinate descent**:

- Particle sizes use stratified quantiles `(i*variants + template_id + 0.5) /
  (particles_per_cluster*variants)` of the configured number PSD. Source shells
  cycle in their explicit input order, with a template offset. Member orientations start fixed locally and can change in the optional target search; accepted clusters can rotate globally. Size parameters still
  mean the sizes of individual particles, not aggregate diameters.
- Sort bounding balls largest first; assign the first N integer lattice sites with
  even `x+y+z`, ordered by radial distance (sphere) or max coordinate magnitude
  (cube), then squared norm and lexicographic coordinates. With coordinate pitch
  `(2*max_radius + safe_gap)/sqrt(2)`, even-parity FCC nearest neighbours are
  separated by at least `2*max_radius + safe_gap`.
- Sweep members in a fixed order. Move each toward the origin, then toward the
  three coordinate planes. For every other bounding ball, solve the first swept
  ray/ball contact; clamp movement to the earliest contact, with a small inward
  margin. Endpoint-only checks are insufficient: a particle must not tunnel
  through another. Stop on the configured sweep count or negligible movement.
- Independently check bounding-ball gaps after initialization. Then optionally run
  `mesh_refinement_sweeps` passes of deterministic coordinate descent on the real
  meshes. For each member, try the origin and three coordinate planes with a
  fixed backtracking sequence 1, 1/2, ..., 1/2048. Accept only placements passing
  exact surface-intersection, containment and surface-gap checks against every
  other member. The new placement must remain within the current container bound.
  This optimizes a static feasible assembly; it does not model a physical trajectory.
- Revalidate every real mesh pair after refinement, including interrupted paths.
  Fit the final sphere/cube to **actual transformed vertices**, not to the unused
  space of member bounding balls. No members are dropped or resized during refinement.

The initial bounding-ball stage is **conservative** for irregular shapes; real-mesh
refinement relaxes that constraint while preserving actual mesh separation. This is
a deterministic local packing method, not a certificate of the densest possible packing. Polydispersity, bounding-ball slack and
finite cluster boundaries can leave significant matrix volume inside the proxy.
Both initial and final true internal VF are reported. Attainable density depends
on particle geometry and packing constraints. Proxy volume and particle material
volume have different meanings. This is a modeling change that
introduces clustered spatial correlations, not a statistically identical speedup
of ordinary RSA.

## Global checks

For sphere proxies, pair clearance is exact centre distance minus both radii.
For cube templates, rotate the cube with the cluster and use its enclosing world
AABB as the collision box. The resulting AABB can be larger; this intentionally
rejects some otherwise feasible arrangements. `orientation.mode: fixed` avoids
this extra inflation. All true member geometry remains inside its proxy.

The domain must contain the whole proxy, including the configured wall gap.
The sampling box is eroded by the sphere radius or cube circumsphere radius,
independent of rotation. Candidate neighbours come from an aggregate-only spatial
grid. `gaps.particle_particle` means the **proxy-to-proxy** gap in this path;
`aggregates.internal_gap` controls only members within the same cluster.

Void checks never use an unchecked near-surface shortcut. First reject a proxy
centre inside the frozen pore. A sphere must have nearest pore-surface distance
at least radius + void gap (also rejects a pore wholly inside the sphere). A cube
AABB is tested as a 12-triangle closed solid against pore intersections, both
containment directions and surface distance. Thus pores inside a cluster proxy
are conservatively excluded, even if they would occupy only matrix between members.

This version accepts only strict boundaries, feasible_uniform position and
forbidden void crossing. Incompatible settings are refused, not silently ignored.

## Accounting, interruption and outputs

- Template count is not global cluster count. Variants are repeated in fixed
  cyclic order. The number of planned clusters is the nearest whole-template
  cumulative volume to the target; member diameters are never rescaled to make
  that count fit. No partial-template top-up is performed; smaller whole templates are used only in explicit mixed mode.
- Per-particle attempt budget now bounds attempts **per cluster**. Report
  `samplers.attempt_unit=cluster_proposal` and `stop_detail` make this explicit.
  Exhausted clusters leave real member-size shortfalls, not replacement draws.
- Ordinary report counts, PSD and VF are all for **individual particles**.
  `aggregates.json` separately records planned/processed clusters, each accepted
  template id, transform, contiguous particle range, and proxy fraction.
- `aggregate_templates.json` records algorithm/config, template geometry source
  identities and local transforms, enclosing size, actual/proxy volumes and
  density before/after settlement. `aggregate_templates/template_NNNN.stl`
  contains the real constituent meshes, not a solid proxy.
- `particles.json` and `particles.stl` remain flat real-particle outputs. Source
  transforms reconstruct each particle directly. Proxy volume is never particle material volume.
- Existing SIGINT/SIGTERM/STOP cancellation also applies. Generation stops at a
  safe member boundary; already built templates remain available. Global placement
  commits a whole accepted cluster before honoring interruption. Standard output
  saving then preserves all accepted particles, plus cluster metadata.
- Progress identifies `generating_templates`, then `packing`, then `saving` and
  final `finished`/`interrupted`. Seed affects only global placement; template
  construction is independent of seed and worker count.

See [configuration](../reference/config.md#aggregate-configuration),
[function contracts](../reference/pipeline-aggregates.md), and
[worked configuration](../examples/pack-aggregates.md).

## Target-driven compaction and mixed stages

`target_internal_volume_fraction` activates a bounded deterministic search after
existing FCC/contact/mesh refinement. Prefer a progressively shrinking sphere or
cube, with desired half-size computed from true material volume and the requested
internal VF. The container is a soft optimization objective during rearrangement;
it is never falsely used to enclose vertices outside it. Final proxy size always
comes from the actual vertices.

Each round tries inward translations, signed axis rotations (alone and coupled
with inward motion), signed lateral translations (alone and coupled), and nearest
neighbour pair translations. Each proposal is accepted only if its support/centre
compression cost decreases and all real solid intersection, nesting and gap checks
pass. Alternate member order, reduce steps every three rounds, and recenter by the
vertex AABB midpoint if that reduces envelope size. Pair motions check both members
against each other and all remaining members. No random positions, resizing,
member deletion, or overlap relaxation occurs. This is a static feasible search,
not a physical motion/contact simulation or a global optimum certificate.

Keep the assembly with smallest enclosing size, including improvements in a
partially interrupted round; restore it and independently validate every pair.
Stop at achieved internal target, `strategy_rounds`, `max_compaction_trials`, or
cooperative interruption. Record per-strategy candidate counters, accepted moves,
completed rounds and stop reason. An unmet internal target does not claim success:
`target_reached=false` and the best template remains usable in global Pack.

Aggregates remain disabled by default. `mode: clusters` keeps the primary-only
path; `mode: mixed` first places primary blocks, then creates plans from remaining
true material volume for successively smaller member counts. Generate the template
catalog before global packing. `variants: auto` uses the source-shell/primary-member
ratio, clamped to 4..32; this is a reproducible diversity heuristic, not an optimal
count estimate. Each mixed stage has that many templates. Configured integers are
honored. The complete catalog is capped at 65536 real members.

Mixed mode reserves equal shares of the remaining finite global proposal budget
for remaining stages. Earlier stages can stop on failure according to the existing
on_unattainable setting; smaller stages can still run. Fallback count=1 places one
real particle using its conservative proxy. Smaller blocks fill space outside
accepted larger proxies; they do not enter matrix voids inside a larger proxy.
Report each stage and the original failed-size shortfall; added fallback draws are
included in plan and size-class counts. Mixed replacement can change the realized
PSD; it is explicit opt-in behavior. Final global target status is separate from
internal template target status. Compose world rotation as global * local so all
flat particle transforms and exported template transforms reconstruct exactly.

Recovery and geometry residency are described in [placement checkpoints and bounded geometry](placement-checkpoints-and-memory.md). Completed templates are reused; only the interrupted current template is deterministically rebuilt. Global cluster attempts and mixed stages resume from their saved cursors.


## Exact fallback into remaining material space

Set `aggregates.exact_fallback: true` with mixed mode to refine smaller-stage
proposals against actual member meshes. The primary stage still uses conservative
proxies. Subsequent stages query the individual-particle spatial grid and reuse
the ordinary boundary, pore containment, surface intersection, both containment
directions, and exact surface-gap checks. A proxy overlap is not a rejection.
Empty matrix space inside a larger aggregate proxy is available for insertion.
The incoming member keeps `gaps.particle_particle` against every existing real
particle; only pairs belonging to the same newly built template use internal_gap.
The default is false; the mixed example enables it. Member material volume remains
the only contribution to VF. Report sampler `aggregate_fallback_collision` states
`exact_members` or `proxies`. Recovery remains tied to the same executable and
physical config. To retain a population from an older executable, use the
explicit validated initial-assembly import described in the checkpoint document.

## Contact growth (`construction: contact_growth`)

The default constructor remains `fcc` for compatibility; the shipped aggregate
and mixed examples explicitly select `contact_growth`. Ordinary Pack remains
disabled for aggregates unless `enabled: true`.

Contact growth fixes stratified diameter quantiles and source-shape assignments,
then inserts members in descending bounding-radius order. It enumerates fixed
spherical directions and deterministic orientations at a provably separated
exterior location, approaching the centre or an existing member. No random
positions, particle deletion, rescaling or overlap relaxation are used. Candidate
ranking first minimizes the actual vertex envelope, then squared centre radii.

Each motion uses conservative advancement: the surface-distance surplus over the
internal gap bounds safe progress. The displacement bound for a combined rigid
motion is `|translation| + bounding_radius * |rotation_angle|`, and every step moves
less than the current surplus, so a swept path cannot cross or tunnel through an
obstacle whose far-side endpoint would otherwise look feasible. `contact_tolerance`
is relative to the moving member's bounding radius; an extra numerical guard keeps
the accepted gap on the separated side. Near-contact tolerance stops approach but
still permits separating motion. Reaching `contact_max_steps` keeps the last feasible
pose, not a claim of exact contact or saturation.

### Geometry kernel and cost

Each member's query mesh and bounding hierarchy are built once, in the member's own
scaled frame, and queried through its rigid pose (an isometry); a move never copies,
transforms or re-indexes geometry. One step costs one distance query per nearby
obstacle: a bounding-ball test rejects far pairs, then a simultaneous traversal of
both hierarchies computes the exact minimum over triangle pairs by branch and bound.
The query is capped at what the step can use (at most half the moving bounding
radius), so far node pairs are pruned early; long motions take a few more, cheaper
steps. The next pose's distance is the next step's bound, so no pose is measured
twice. Nesting needs no test during motion: every motion starts from a separated
pose and moves less than the clearance. Envelopes use each body's convex-hull
vertices, which give exactly the all-vertex envelope.

Independent work runs on the Rayon pool: the direction x orientation approaches of
one insertion, and the six transactions that relax one member, all start from the
same arrangement and are ranked in a fixed order, so results do not depend on the
thread count. Consecutive templates of one stage are generated concurrently (up to
the thread count) by `run_growth_batch`.

The final template is certified independently with the world-frame exact predicates
(intersection, nesting and gap) before export.

### Optional search controls

`contact_orientation: principal` replaces the fixed start rotations by orientations
that lay a member's minor (or major) principal axis along the approach direction,
spun about it, so flat faces meet the assembly. `contact_settle_steps > 0` rolls the
best `contact_settle_candidates` straight approaches toward the centre (lift slightly,
slide tangentially, fall), accepting only strictly closer feasible poses. Both are off
by default; with them off the method is the original one.

After insertion, `neighborhood_sweeps` visits the current assembly. A transaction
can retreat, rotate/slide, and approach again, temporarily increasing its envelope
without allowing overlap. Optional pair rearrangement settles the nearest neighbour
after the candidate move and judges the combined result. Rotations refine by sweep;
the entire transaction is accepted only if envelope/centre score improves.
When an internal VF target is configured, the final assembly also gets up to
`strategy_rounds` additional sweeps. A deterministic centre search tightens sphere
or cube bounds; all final vertices are still enclosed. This is geometric packing,
not a force-equilibrium simulation or a global density optimum certificate.

`max_compaction_trials` bounds insertion and rearrangement candidates together;
future insertion candidates are reserved before relaxation. If this budget is
insufficient, remaining planned members get valid exterior fallback placements;
none disappear and an unmet target is reported. Each candidate has separately
bounded motion steps. FCC-only controls are `compaction_sweeps`,
`mesh_refinement_sweeps`, and `container_shrink_fraction`.

Checkpoints carry, for every template of the current concurrent batch, the pending
member plan, committed members, best arrangement, next insertion, relaxation
member/sweep and counters. Periodic saves occur at batch boundaries; an interrupt
rolls back only each template's unfinished insertion or transaction and forces the
usual checkpoint save. A hard kill loses at most the current batch's progress. Resume replays that unit rather
than rebuilding the template. Incomplete templates are visualization artifacts
and never enter global packing. A final independent pair check precedes export.
The completed-template and global mixed-stage resume contracts are unchanged.

Stop reasons distinguish target attainment, trial exhaustion, interruption and
completed relaxation sweeps. A higher template VF does not guarantee any global
Pack target. Finite direction/orientation searches can miss usable contacts.
