# Enable and tune aggregate Pack

将下面的块添加到已有的、已校验的放置配置中的 `placement:` 之下：

```yaml
  aggregates:
    enabled: true
    construction: contact_growth # fcc retains legacy construction
    contact_directions: 12
    contact_orientations: 4
    contact_tolerance: 0.00001
    contact_max_steps: 96
    neighborhood_sweeps: 2
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

`size.distribution` 仍作为单个颗粒的 PSD。全局的壁面间隙与孔隙间隙仍保留在其原有字段中。启用后，
`gaps.particle_particle` 即为容器与容器之间的间隙。对于不做 AABB 膨胀的轴对齐立方体，还需使用
`orientation: {mode: fixed}`。若要关闭，请设置 `enabled: false` 或删除该块；此后其他流水线选项恢复其原有含义。

运行常规 CLI：

```bash
rustmspt pack --config placement.yaml --threads 4
```

通过 `aggregate_templates.json` 与模板 STL 检查内部密度，通过 `aggregates.json` 检查各实例及其所含单个颗粒的范围。
真实 VF 仍记录在 `run_report.json` 中。目标可能因代理体拥塞或整模板体积的粒度而未能达成。标准的 STOP 文件或
Unix 下的 Ctrl-C 会将已接受的团簇保存为真实的颗粒输出。

这些设置仅用于说明配置接口。可达到的密度取决于输入几何与分离约束。关于保守性与所支持的模式，请阅读
[算法文档](../algorithms/aggregate-placement.md)。

内部目标 0.50 是一次搜索请求，而非有保证的结果。全局 VF 仍来自 `placement.target.volume_fraction`。
普通颗粒使用 `enabled: false`；纯初级团簇使用 `enabled: true, mode: clusters`；有序的大/小回退使用
`enabled: true, mode: mixed`。请检查每个模板的 target_reached 与 compaction_search，以及 aggregates.json 中的各阶段。

仓库模板随附于 data/input/placement_config.yaml（已禁用）、placement_void_config.yaml（已禁用）、
placement_aggregate_config.yaml（clusters）与 placement_mixed_config.yaml（mixed）。所有模板都显式列出了新的控制项。
配置模板回归测试会解析每个随附模板。

使用接触生长时，检查点还会保留当前模板内已提交的成员。`max_compaction_trials` 包含插入与松弛候选；`strategy_rounds` 追加以目标为导向的最终扫描。两个 FCC 扫描字段与 container_shrink_fraction 在该构造器中不使用。需要更大的目录时请使用显式的模板数量；现有的 auto 启发式保持不变。
