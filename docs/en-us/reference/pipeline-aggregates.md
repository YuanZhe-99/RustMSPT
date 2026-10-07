# Aggregate placement reference

Read [the algorithm](../algorithms/aggregate-placement.md) before these contracts.
The implementation is `src/pipeline/placement_aggregates.rs`, a child of the
placement engine so it shares normal particle acceptance and output writers.

| Item | Contract |
|---|---|
| `Member` | Source-shell index, diameter, scale, bounding radius, local centre and local quaternion; no proxy counted as material. |
| `Template` | Fixed members, enclosing half-size/radius, summed material volume, initial extent, completed sweeps and SearchStats. |
| `Proxy` | Accepted cluster centre, sphere radius and world AABB for cheap global queries. |
| `xyz` | Convert Vec3 to serialized XYZ coordinates. |
| `envelope` | Bound all actual member vertices by an origin-centred sphere/cube. |
| `fcc_sites` | Deterministic even-parity integer sites, sorted for the chosen container. No RNG. |
| `settle_one` | Move one ball no farther than its first swept contact; mutates its centre, returns travel. |
| `build_template` | Quantile-sized, cyclic-shape FCC initialization, fixed-order contact sweeps, optional real-mesh refinement and independent pair-gap validation. Polls cancellation. |
| `world_proxy` | Sphere or enclosing world AABB of a rotated cube, without expanding real member meshes. |
| `proxy_check` | Domain, aggregate-neighbour and unconditional pore-containment/surface checks; returns a named rejection. |
| `export_template` | Write real template STL; return reconstruction and density JSON. |
| `plan_templates` | Closest cumulative true volume with whole repeated templates and a 5-million-member cap; no PSD rescaling. |
| `run` | Generate templates, place proxies, commit accepted real members, save metadata and standard outputs, including interrupted results. |
| `AggregateSpec::default` | Disabled; variants=8, particles_per_cluster=64, sphere, internal_gap=0.1, compaction_sweeps=32, mesh_refinement_sweeps=8. |
| `SizeSource::quantile` | Evaluate finite u in [0,1), no RNG; sample delegates to this using the original one-u64 draw. |

`PlacementParams::validate` validates opt-in bounds and mode compatibility, and
copies the aggregate config into `ResolvedPlacement`. `run_placement_in_pool`
branches only if enabled. `EngineState::new` uses one cell for empty plans to
avoid a degenerate grid after cancellation before generation. `PlacementControl`
now carries a phase label; polling, signal and saving semantics are unchanged.

Output limit notes: templates have at most 1024 members and 128 variants, with
at most 65536 catalog members, 1024 bounding-ball sweeps and 128 mesh-refinement sweeps. Accepted world geometry is reconstructed through a bounded cache; final particle
STL streams one reconstructed member at a time. Global placement is sequential; the configured
worker pool still covers source loading and standard optional output generation.


| Mesh-refinement item | Contract |
|---|---|
| `PreparedMember` | One current real mesh, bounding box and cached query hierarchy. |
| `prepare_member` | Build a member's exact scaled/translated geometry and collision shape; reject unusable shapes. |
| `members_clear` | AABB reject followed by solid intersection/nesting and surface-gap predicates. |
| `refine_meshes` | Fixed-order origin/axis-plane coordinate descent with deterministic backtracking, no random positions; preserve container bound and every member; all-pairs check after completion/cancellation. |

| Target-search item | Contract |
|---|---|
| `SearchStats` | Completed rounds, candidate/strategy/accepted counters and target/interrupted/budget stop reason. |
| `compose_rotation` | Normalized global*local quaternion; preserves transform reconstruction. |
| `container_volume` | True enclosing sphere/cube volume for one half-size. |
| `compression_cost` | Squared outer support plus shrinking-container overshoot penalty and small centre-norm penalty. |
| `recenter` | Rigid midpoint translation only if actual enclosing size decreases; preserves pair clearance. |
| `compact_to_target` | Optional bounded deterministic inward, rotation, lateral and pair search; best feasible assembly preserved and independently validated, including interrupted paths. |
| `deserialize_aggregate_variants` | Positive integer or `auto` sentinel; refuses numeric zero and arbitrary strings. |

`AggregateSpec` adds optional internal target and configurable strategies/budgets;
new controls and mixed catalog bounds are validated before loading geometry.
`run` resolves auto variants, builds primary/fallback catalogs, reserves global
attempt budget per mixed stage, appends explicit fallback draws to plan statistics,
and records each stage plus template target shortfalls. Default mode is clusters,
default feature enabled=false. Local rotations apply in envelope, prepared collision
geometry, template STL, template metadata and global flat particle output.

## Additional regression contracts

| Test | Independent contract |
|---|---|
| `target_search_rotates_rearranges_and_preserves_real_geometry` | Anisotropic sources, all strategies, nonidentity local rotations, exported world reconstruction, gap and thread determinism. |
| `target_search_reports_success_budget_and_invalid_controls` | Target success versus budget shortfall and invalid finite/range controls. |
| `automatic_templates_and_mixed_fallback_place_smaller_blocks` | Auto count, smaller-stage placement, budget and draw accounting, real cross-stage gap, explicit small catalog bounds. |
| `shipped_configuration_templates_expose_supported_modes` | Shipped default/void/cluster/mixed YAML resolve with all supported controls. |
| `stop_during_target_search_preserves_valid_best_template` | Stop mid-search saves best template, standard interrupted outputs and independent all-pair mesh gap verification. |

These regression contracts are implemented in `tests/placement_aggregate_tests.rs`.

## Resumable aggregate state

`AggregateCursor` stores completed templates/catalogs, generation cursors,
current mixed stage and its reserved budget, template-ID plan, cluster attempt
cursor, proxies, membership and stage accounting. `save_aggregate` serializes only
at committed safe points when due or forced. `run` restores the proxy index in
acceptance order and re-exports templates into the selected output directory.
An interrupted current template remains a visualization artifact and is rebuilt
deterministically; completed templates are not rebuilt. Global attempts resume
exactly. Completed snapshots remain available for explicit target extension.


| Additional item | Contract |
|---|---|
| `exact_fallback_check` | For every proposed smaller-stage member, query actual particles and reuse ordinary exact feasibility, preserving member-local internal gaps; never reject merely because proxies overlap. |

Mixed `exact_fallback` queries use a rebuilt individual-particle grid after
checkpoint restoration. Proxy-only placement remains the default.

## Contact-growth contracts

Implementation: `src/pipeline/placement_aggregate_contact.rs` (search) and `src/pipeline/placement_aggregate_bodies.rs` (posed geometry kernel).

| Item | Contract |
|---|---|
| `Growth` | Serializable pending plan, committed/best members and insertion/relaxation cursors; the posed-body cache is not serialized and is rebuilt on resume. |
| `Body` / `Bodies` | Per-member query mesh/hierarchy in its own scaled frame plus convex-hull vertices, principal axes and volume, keyed by shell and exact scale; built once, never per pose. |
| `Bodies::ensure` / `pose` | Build missing bodies; pose a member as an isometry without copying or transforming geometry. |
| `Bodies::envelope` | Sphere/cube envelope from hull vertices; equal to the all-vertex envelope (both norms are convex). |
| `principal_axes` | Vertex-covariance eigenvectors, longest extent first. |
| `distance_capped` | min(exact triangle-pair surface distance, cap) by simultaneous branch-and-bound hierarchy traversal through the relative isometry; bounding-ball rejection first. Nesting is not tested (motions start separated and move conservatively). |
| `clearance_capped` | Smallest capped distance to a set of posed obstacles, tightening the cap as it goes. |
| `directions` | Return deterministic spherical directions without RNG or thread dependence. |
| `orientation` | Start orientation per approach: legacy fixed rotations, or (`contact_orientation: principal`) minor/major principal axis along the approach with deterministic spins. |
| `advance` | Conservative advancement of a translation/rotation: each step moves less than the current exact clearance surplus, one capped distance query per step (cap <= half the bounding radius keeps pruning tight); returns last feasible pose on contact, step limit or cancellation. |
| `settle` | Optional roll toward the centre (lift, tangential slide, fall); accepts only strictly closer feasible poses. |
| `insertion_search` | Straight approaches over directions x orientations evaluated in parallel, stable ranking, optional settling of the best `contact_settle_candidates`; always returns a feasible pose. |
| `parallel` | Run independent motion jobs on the Rayon pool with cancellation-only worker controls; results keep job order, so output is thread-count independent. |
| `centre_envelope` | Bounding-box centring then deterministic pattern search over hull vertices; rigidly translate all members only when the envelope shrinks. |
| `score` | Whole-template envelope and squared-centre secondary cost, no mutation. |
| `better` | Lexicographic acceptance of envelope then centre cost. |
| `Growth::new` | Fix quantile sizes/source assignments and initialize serializable construction state. |
| `Growth::step` | Commit one insertion or relaxation transaction (the six relaxation transactions of a member run in parallel from the same arrangement); reserve future insertion budget; rollback unfinished unit on stop; accumulates profiling counters. |
| `Growth::target_reached` | Compare best-member material volume to the real enclosing proxy volume. |
| `Growth::snapshot` | Independently validate every pair with the world-frame exact predicates and export committed members with honest target/stop status. |
| `run_growth_batch` | Advance up to `threads` consecutive contact-growth templates of one stage concurrently; heartbeat thread keeps progress fresh; stop latches into the run control. |
| `commit_template` | Append a completed template, export it and advance the variant/stage cursor. |
| `contact::tests::setup` | Build a closed-mesh motion/serialization fixture. |
| `contact_advance_stops_before_obstacle_even_when_endpoint_is_clear` | Pin first-contact stopping, surface gap and separating-motion escape. |
| `posed_distance_matches_world_frame_distance` | Posed capped distance equals the world-frame exact distance for rotated/scaled members, respects the cap; hull envelope equals all-vertex envelope. |
| `contact_growth_serialized_member_boundary_resumes_exactly` | Require identical final transforms/counters (wall time excluded) after serialized intermediate state. |
| `contact_growth_geometry_density_and_determinism` | Check exported clearances/transforms, density improvement and thread determinism. |
| `contact_growth_controls_and_exhaustion` | Reject invalid controls and preserve all members at zero search budget. |

`AggregateCursor::batch` holds the concurrently generated in-progress templates (an older single `growth` entry is resumed as the first batch member). `run` saves at batch boundaries and on stop, and admits only completed templates. `AggregateConstruction` selects the legacy FCC or contact-growth path. Old FCC partial-template rebuilding semantics apply only to FCC.

Additional regressions: `contact_rotation_cannot_tunnel_with_clear_endpoints` checks swept bar rotation, and `contact_growth_cli_stop_resume_matches_uninterrupted` checks saved in-template state plus binary-identical resumed geometry.
