//! Island-local geometric VF contributions; never used for voxel S2 or placement.
use crate::geometry::{particle_volume_in_bbox, split_mesh_into_granules};
use crate::types::{BoundingBox, Mesh};

pub(super) struct IslandVolumes {
    components: Vec<Vec<f64>>,
    bbox: BoundingBox,
}

impl IslandVolumes {
    // AI-FUNC-SUMMARY: Cache each particle's connected-component clipped volumes in source order; no clamping until the complete population is summed.
    pub fn new<'a>(meshes: impl Iterator<Item = &'a Mesh>, bbox: BoundingBox) -> Self {
        Self {
            components: meshes.map(|mesh| Self::contributions(mesh, bbox)).collect(),
            bbox,
        }
    }

    // AI-FUNC-SUMMARY: Compute the same connected-component contributions as the merged-mesh VF reference, retaining component order and the empty-split fallback.
    fn contributions(mesh: &Mesh, bbox: BoundingBox) -> Vec<f64> {
        let parts = split_mesh_into_granules(mesh);
        if parts.is_empty() {
            vec![particle_volume_in_bbox(mesh, bbox)]
        } else {
            parts
                .iter()
                .map(|part| particle_volume_in_bbox(part, bbox))
                .collect()
        }
    }

    // AI-FUNC-SUMMARY: Recompute only a changed particle and return its prior contribution vector for exact rejection rollback.
    pub fn replace(&mut self, index: usize, mesh: &Mesh) -> Vec<f64> {
        std::mem::replace(
            &mut self.components[index],
            Self::contributions(mesh, self.bbox),
        )
    }

    // AI-FUNC-SUMMARY: Restore saved contributions after a rejected candidate without clipping or resumming geometry.
    pub fn restore(&mut self, index: usize, old: Vec<f64>) {
        self.components[index] = old;
    }

    // AI-FUNC-SUMMARY: Sum cached components in merged source order and apply the original box-volume floor/clamp; no running delta accumulation or drift.
    pub fn fraction(&self) -> f64 {
        let total: f64 = self.components.iter().flatten().copied().sum();
        (total / self.bbox.volume().max(1e-12)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{box_mesh, merge_meshes, translate_mesh, volume_fraction_in_bbox};
    use crate::types::Vec3;
    // AI-FUNC-SUMMARY: Compare every accepted/rejected translation, multi-component particle, full refresh, removal and migration against merged geometric VF.
    #[test]
    fn cached_contributions_match_reference_and_rollback() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let cube = box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.25, 0.25),
            max: Vec3::new(1.25, 1.25, 1.25),
        });
        let mut second = cube.clone();
        translate_mesh(&mut second, Vec3::new(2.0, 0.0, 0.0));
        let mut parts = vec![cube.clone(), merge_meshes(&[cube.clone(), second])];
        let mut cache = IslandVolumes::new(parts.iter(), bbox);
        for step in 0..160 {
            let index = step % parts.len();
            let original = parts[index].clone();
            translate_mesh(
                &mut parts[index],
                Vec3::new(if step % 2 == 0 { 0.137 } else { -0.19 }, 0.0, 0.0),
            );
            let old = cache.replace(index, &parts[index]);
            assert_eq!(
                cache.fraction(),
                volume_fraction_in_bbox(&merge_meshes(&parts), bbox)
            );
            if step % 3 == 0 {
                parts[index] = original;
                cache.restore(index, old);
            }
            if step % 64 == 63 {
                cache = IslandVolumes::new(parts.iter(), bbox);
            }
            assert_eq!(
                cache.fraction(),
                volume_fraction_in_bbox(&merge_meshes(&parts), bbox)
            );
        }
        parts.remove(0);
        cache = IslandVolumes::new(parts.iter(), bbox);
        assert_eq!(
            cache.fraction(),
            volume_fraction_in_bbox(&merge_meshes(&parts), bbox)
        );
        parts = vec![cube];
        cache = IslandVolumes::new(parts.iter(), bbox);
        assert_eq!(
            cache.fraction(),
            volume_fraction_in_bbox(&merge_meshes(&parts), bbox)
        );
    }

    // AI-FUNC-SUMMARY: Verify cached VF leaves fixed-seed continuous MC curves exactly unchanged, including the radius-zero shortcut.
    #[test]
    fn cached_vf_preserves_seeded_mc() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let parts = vec![
            box_mesh(BoundingBox {
                min: Vec3::new(-0.2, 0.3, 0.3),
                max: Vec3::new(1.3, 1.3, 1.3),
            }),
            box_mesh(BoundingBox {
                min: Vec3::new(2.2, 2.2, 2.2),
                max: Vec3::new(3.2, 3.2, 3.2),
            }),
        ];
        let merged = merge_meshes(&parts);
        let cache = IslandVolumes::new(parts.iter(), bbox);
        for seed in [0, 42] {
            for r_max in [0, 3] {
                assert_eq!(
                    crate::geometry::s2::calculate_s2_mesh_mc_seeded(
                        &merged, bbox, r_max, 257, seed, true
                    ),
                    crate::geometry::s2::calculate_s2_mesh_mc_seeded_with_vf(
                        &merged,
                        bbox,
                        r_max,
                        257,
                        seed,
                        true,
                        cache.fraction()
                    )
                );
            }
        }
    }

    // AI-FUNC-SUMMARY: Compare full merged-mesh VF against changed-particle caching including each 64-step full refresh, with identical transforms and per-step fractions.
    #[test]
    #[ignore = "release SA contribution-cache benchmark"]
    fn volume_cache_benchmark() {
        use std::time::Instant;
        let bbox = BoundingBox::from_size(Vec3::new(20.0, 20.0, 20.0));
        for count in [1usize, 32, 512] {
            let initial: Vec<_> = (0..count)
                .map(|i| {
                    let min = Vec3::new(
                        (i % 16) as f64 + 0.1,
                        ((i / 16) % 16) as f64 + 0.1,
                        (i / 256) as f64 + 0.1,
                    );
                    box_mesh(BoundingBox {
                        min,
                        max: min.add(Vec3::new(0.4, 0.4, 0.4)),
                    })
                })
                .collect();
            let run = |cached: bool| {
                let mut parts = initial.clone();
                let mut merged = merge_meshes(&parts);
                let mut cache = IslandVolumes::new(parts.iter(), bbox);
                let mut values = Vec::with_capacity(100);
                let start = Instant::now();
                for step in 0..100 {
                    let index = step % count;
                    translate_mesh(
                        &mut parts[index],
                        Vec3::new(if step % 2 == 0 { 0.001 } else { -0.001 }, 0.0, 0.0),
                    );
                    merged.vertices[index * 8..(index + 1) * 8]
                        .copy_from_slice(&parts[index].vertices);
                    if cached {
                        cache.replace(index, &parts[index]);
                        if step % 64 == 63 {
                            cache = IslandVolumes::new(parts.iter(), bbox);
                        }
                        values.push(cache.fraction());
                    } else {
                        values.push(volume_fraction_in_bbox(&merged, bbox));
                    }
                }
                (start.elapsed().as_secs_f64(), values)
            };
            assert_eq!(run(false).1, run(true).1);
            for sample in 0..5 {
                let (old, new) = if sample % 2 == 0 {
                    (run(false), run(true))
                } else {
                    let new = run(true);
                    (run(false), new)
                };
                assert_eq!(old.1, new.1);
                eprintln!("SA_VF_BENCH particles={count} sample={sample} evaluations=100 old_seconds={:.9} new_seconds={:.9}",old.0,new.0);
            }
        }
    }
}
