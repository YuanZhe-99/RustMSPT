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
at most 65536 catalog members, 1024 bounding-ball sweeps and 128 mesh-refinement sweeps. Memory still scales with accepted
real geometry for final STL export. Global placement is sequential; the configured
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

All tests live in tests/placement_aggregate_tests.rs; its 13 tests and 108 other
placement tests pass. Existing unrelated Clippy warnings are retained.
