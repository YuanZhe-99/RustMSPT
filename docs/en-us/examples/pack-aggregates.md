# Enable and tune aggregate Pack

Add this block beneath `placement:` in an existing validated placement config:

```yaml
  aggregates:
    enabled: true
    mode: mixed                 # clusters or mixed; ignored when disabled
    variants: auto              # Or an explicit integer, e.g. 8
    target_internal_volume_fraction: 0.50
    strategy_rounds: 8
    max_compaction_trials: 6000
    rotation_search: true
    lateral_rearrangement: true
    pair_rearrangement: true
    container_shrink_fraction: 0.03
    rotation_step_degrees: 15
    fallback_particles_per_cluster: [16, 4, 1]
    particles_per_cluster: 64   # Members in each template
    shape: sphere               # sphere or cube
    internal_gap: 0.1           # Same length unit as particle dimensions
    compaction_sweeps: 32       # Bounding-ball contact settlement
    mesh_refinement_sweeps: 8   # Real-mesh constrained coordinate descent
```

Keep `size.distribution` as the individual-particle PSD. Keep global wall and pore
gaps in their existing fields. `gaps.particle_particle` is the container-to-container
gap when enabled. For axis-aligned cubes without AABB inflation, also use
`orientation: {mode: fixed}`. To switch off, set `enabled: false` or remove the
block; other pipeline options then have their original meanings.

Run the normal CLI:

```bash
rustmspt pack --config placement.yaml --threads 4
```

Inspect `aggregate_templates.json` and template STLs for internal density, and
`aggregates.json` for instances and their individual-particle ranges. True VF is
still in `run_report.json`. A target can be missed through proxy congestion or
whole-template volume granularity. The standard STOP file or Unix Ctrl-C saves
accepted clusters as real particle outputs.

These settings illustrate the configuration interface. Attainable density depends
on the input geometry and separation constraints. Read [the algorithm](../algorithms/aggregate-placement.md)
for conservatism and supported modes.

Internal target 0.50 is a search request, not a guaranteed result. The global VF
still comes from `placement.target.volume_fraction`. For ordinary particles use
`enabled: false`; for pure primary clusters use `enabled: true, mode: clusters`;
for ordered large/small fallback use `enabled: true, mode: mixed`. Inspect each
template's target_reached and compaction_search, and aggregates.json stages.

Repository templates are shipped in data/input/placement_config.yaml (disabled),
placement_void_config.yaml (disabled), placement_aggregate_config.yaml (clusters)
and placement_mixed_config.yaml (mixed). All expose the new controls explicitly.
The configuration-template regression resolves each shipped template.
