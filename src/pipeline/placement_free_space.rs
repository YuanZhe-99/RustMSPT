//! Bounded, deterministic cavity guidance. Scores never replace exact feasibility.
use super::*;
use crate::geometry::collision::trimesh_contains_point;
use crate::geometry::vec_norm;
use crate::types::BoundingBox;
use parry3d_f64::na::Point3;
use parry3d_f64::query::PointQuery;
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;

#[derive(Clone, Serialize, Deserialize)]
struct Cell {
    bbox: BoundingBox,
    root: usize,
    active: bool,
    dirty: bool,
    clearance: f64,
    failures: usize,
    visits: usize,
    version: u64,
}

#[derive(Clone)]
struct Entry {
    priority: f64,
    id: usize,
    version: u64,
}
impl PartialEq for Entry {
    fn eq(&self, b: &Self) -> bool {
        self.priority.to_bits() == b.priority.to_bits()
            && self.id == b.id
            && self.version == b.version
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, b: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(b))
    }
}
impl Ord for Entry {
    fn cmp(&self, b: &Self) -> std::cmp::Ordering {
        self.priority
            .total_cmp(&b.priority)
            .then_with(|| b.id.cmp(&self.id))
            .then(self.version.cmp(&b.version))
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Stats {
    pub score_queries: usize,
    pub guided_proposals: usize,
    pub exploration_proposals: usize,
    pub guided_accepted: usize,
    pub exploration_accepted: usize,
    pub refinements: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Index {
    cells: Vec<Cell>,
    dimensions: [usize; 3],
    coarse_size: f64,
    penalty_scale: f64,
    capacity: usize,
    draw_key: String,
    pub stats: Stats,
    #[serde(skip)]
    heap: BinaryHeap<Entry>,
    #[serde(skip)]
    roots: Vec<Vec<usize>>,
}

// AI-FUNC-SUMMARY: Return a box centre without geometry queries.
fn midpoint(b: BoundingBox) -> Vec3 {
    b.min.add(b.max).scale(0.5)
}

impl Index {
    // AI-FUNC-SUMMARY: Allocate a bounded coarse lattice of unknown cells; real scores are evaluated lazily and interruption remains cheap.
    pub fn new(config: &ResolvedPlacement) -> Result<Self> {
        let spec = &config.position.free_space;
        let size = config.domain.size();
        let dimensions =
            [size.x, size.y, size.z].map(|n| (n / spec.coarse_cell_size).ceil() as usize);
        let count = dimensions
            .iter()
            .try_fold(1usize, |a, b| a.checked_mul(*b))
            .ok_or_else(|| RustMsptError::InvalidConfig("free-space dimensions overflow".into()))?;
        // Includes cell, stale heap allowance, root memberships and allocator slack.
        let capacity = spec
            .max_cells
            .min(spec.index_memory_mb.saturating_mul(1024 * 1024) / 512);
        if count == 0 || count > capacity {
            return Err(RustMsptError::InvalidConfig("free-space coarse lattice exceeds max_cells/index_memory_mb; increase coarse_cell_size".into()));
        }
        let mut index = Self {
            cells: Vec::with_capacity(count),
            dimensions,
            coarse_size: spec.coarse_cell_size,
            capacity,
            penalty_scale: spec.min_cell_size,
            draw_key: String::new(),
            stats: Stats::default(),
            heap: BinaryHeap::new(),
            roots: vec![vec![]; count],
        };
        for x in 0..dimensions[0] {
            for y in 0..dimensions[1] {
                for z in 0..dimensions[2] {
                    let lo = config
                        .domain
                        .min
                        .add(Vec3::new(x as f64, y as f64, z as f64).scale(index.coarse_size));
                    let hi = lo.add(Vec3::new(
                        index.coarse_size,
                        index.coarse_size,
                        index.coarse_size,
                    ));
                    let bbox = BoundingBox {
                        min: lo,
                        max: Vec3::new(
                            hi.x.min(config.domain.max.x),
                            hi.y.min(config.domain.max.y),
                            hi.z.min(config.domain.max.z),
                        ),
                    };
                    index.add(bbox, index.cells.len(), 2.0 * index.coarse_size);
                }
            }
        }
        Ok(index)
    }

    // AI-FUNC-SUMMARY: Insert an active uncertain cell, preserving stable integer identity.
    fn add(&mut self, bbox: BoundingBox, root: usize, clearance: f64) {
        let id = self.cells.len();
        self.cells.push(Cell {
            bbox,
            root,
            active: true,
            dirty: true,
            clearance,
            failures: 0,
            visits: 0,
            version: 0,
        });
        self.roots[root].push(id);
        self.push(id);
    }

    // AI-FUNC-SUMMARY: Rank a cell by real centre clearance, remaining subcell opportunity and current-draw failures; no cell is permanently declared occupied.
    fn priority(&self, id: usize) -> f64 {
        let c = &self.cells[id];
        c.clearance + 0.15 * vec_norm(c.bbox.size()) - 0.5 * self.penalty_scale * c.failures as f64
    }

    fn push(&mut self, id: usize) {
        let c = &self.cells[id];
        if c.active {
            self.heap.push(Entry {
                priority: self.priority(id),
                id,
                version: c.version,
            });
        }
    }

    // AI-FUNC-SUMMARY: Rebuild derived heap and memberships deterministically after restore; geometry caches are intentionally absent.
    pub fn rebuild(&mut self) {
        self.heap.clear();
        self.roots = vec![vec![]; self.dimensions.iter().product()];
        for id in 0..self.cells.len() {
            self.roots[self.cells[id].root].push(id);
            self.push(id);
        }
    }

    // AI-FUNC-SUMMARY: Reset temporary search penalties for each logical draw; retained geometric scores remain valid.
    pub fn begin_draw(&mut self, key: String) {
        if self.draw_key != key {
            self.draw_key = key;
            for c in &mut self.cells {
                c.failures = 0;
                c.visits = 0;
            }
            self.heap.clear();
            for id in 0..self.cells.len() {
                self.push(id);
            }
        }
    }

    // AI-FUNC-SUMMARY: Estimate signed physical clearance at one centre using real meshes and pores, capped spatial queries, and existing bounded geometry cache. This is guidance only.
    fn score(
        config: &ResolvedPlacement,
        placed: &[PlacedParticle],
        grid: &SpatialGrid,
        void: Option<&VoidIndex>,
        point: Vec3,
    ) -> f64 {
        let cap = 2.0 * config.position.free_space.coarse_cell_size;
        let d = config.domain.expanded(-config.boundary.min_boundary_dist);
        let mut result = [
            point.x - d.min.x,
            d.max.x - point.x,
            point.y - d.min.y,
            d.max.y - point.y,
            point.z - d.min.z,
            d.max.z - point.z,
            cap,
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min);
        if let Some(v) = void {
            let distance = v.surface_distance(point);
            let signed = if v.contains_point(point) {
                -distance
            } else {
                distance
            };
            result = result.min(signed - config.void.as_ref().map_or(0.0, |v| v.gap));
        }
        let point_box = BoundingBox {
            min: point,
            max: point,
        };
        let nearby = grid.query_neighbors_with_margin(
            point_box,
            cap + config.gap_particle_particle,
            usize::MAX,
        );
        let p = Point3::new(point.x, point.y, point.z);
        for id in nearby {
            let other = &placed[id];
            let bound = crate::geometry::bbox_distance(point_box, other.bbox);
            if bound - config.gap_particle_particle >= result.max(0.0) {
                continue;
            }
            let prepared = other.prepared();
            if let Some(shape) = &prepared.shape {
                let distance = (shape.project_local_point(&p, false).point - p).norm();
                let signed =
                    if other.bbox.contains_point(point) && trimesh_contains_point(shape, point) {
                        -distance
                    } else {
                        distance
                    };
                result = result.min(signed - config.gap_particle_particle);
            }
        }
        result
    }

    // AI-FUNC-SUMMARY: Select a scored cavity and bounded perturbation or explicit global exploration. All variates are consumed before any exact feasibility check.
    pub fn propose(
        &mut self,
        config: &ResolvedPlacement,
        placed: &[PlacedParticle],
        grid: &SpatialGrid,
        void: Option<&VoidIndex>,
        rng: &mut ChaCha12Rng,
        centre_box: BoundingBox,
        reach: f64,
        mut stopped: impl FnMut() -> bool,
    ) -> Option<(Vec3, Option<usize>)> {
        let branch = u01(rng);
        let jitter = [u01(rng), u01(rng), u01(rng)];
        let uniform = |b: BoundingBox| {
            Vec3::new(
                b.min.x + (b.max.x - b.min.x) * jitter[0],
                b.min.y + (b.max.y - b.min.y) * jitter[1],
                b.min.z + (b.max.z - b.min.z) * jitter[2],
            )
        };
        if branch < config.position.free_space.exploration_fraction {
            self.stats.exploration_proposals += 1;
            return Some((uniform(centre_box), None));
        }
        while let Some(entry) = self.heap.pop() {
            let id = entry.id;
            let c = &self.cells[id];
            if !c.active || c.version != entry.version {
                continue;
            }
            if c.dirty {
                if self.stats.score_queries % 64 == 0 && stopped() {
                    self.push(id);
                    return None;
                }
                let clearance = Self::score(config, placed, grid, void, midpoint(c.bbox));
                self.stats.score_queries += 1;
                let c = &mut self.cells[id];
                c.clearance = clearance;
                c.dirty = false;
                c.version += 1;
                self.push(id);
                continue;
            }
            let c = &self.cells[id];
            let origin = midpoint(c.bbox);
            let span = c.bbox.size().scale(0.5);
            let representative = c.visits % config.position.free_space.candidates_per_location == 0;
            let clamp = |p: Vec3| {
                Vec3::new(
                    p.x.clamp(centre_box.min.x, centre_box.max.x),
                    p.y.clamp(centre_box.min.y, centre_box.max.y),
                    p.z.clamp(centre_box.min.z, centre_box.max.z),
                )
            };
            let mut point = clamp(origin);
            if representative {
                // Coarse centres can straddle a thin cavity. Six bounded face probes
                // improve this representative without spending hidden geometry proposals.
                // These score queries are counted; the small pass is an atomic safe-point unit.
                let mut best = Self::score(config, placed, grid, void, point);
                self.stats.score_queries += 1;
                for offset in [
                    Vec3::new(span.x, 0.0, 0.0),
                    Vec3::new(-span.x, 0.0, 0.0),
                    Vec3::new(0.0, span.y, 0.0),
                    Vec3::new(0.0, -span.y, 0.0),
                    Vec3::new(0.0, 0.0, span.z),
                    Vec3::new(0.0, 0.0, -span.z),
                ] {
                    let candidate = clamp(origin.add(offset));
                    let score = Self::score(config, placed, grid, void, candidate);
                    self.stats.score_queries += 1;
                    if score > best {
                        best = score;
                        point = candidate;
                    }
                }
            } else {
                let safe = (c.clearance - reach).max(0.0);
                let factor = if safe > 0.0 {
                    (safe / vec_norm(span).max(1e-9)).min(1.0)
                } else {
                    1.0
                };
                point = clamp(
                    origin.add(
                        Vec3::new(
                            (2.0 * jitter[0] - 1.0) * span.x,
                            (2.0 * jitter[1] - 1.0) * span.y,
                            (2.0 * jitter[2] - 1.0) * span.z,
                        )
                        .scale(factor),
                    ),
                );
            }
            self.cells[id].visits += 1;
            self.stats.guided_proposals += 1;
            return Some((point, Some(id)));
        }
        self.stats.exploration_proposals += 1;
        Some((uniform(centre_box), None))
    }

    // AI-FUNC-SUMMARY: Commit a real rejection or acceptance; failed sites may subdivide, never exclude an entire AABB as material.
    pub fn feedback(
        &mut self,
        config: &ResolvedPlacement,
        selected: Option<usize>,
        accepted: bool,
    ) {
        if accepted {
            if selected.is_some() {
                self.stats.guided_accepted += 1;
            } else {
                self.stats.exploration_accepted += 1;
            }
        }
        let Some(id) = selected else {
            return;
        };
        let can_grow = self.cells.len().saturating_add(8) <= self.capacity;
        let c = &mut self.cells[id];
        if !accepted {
            c.failures += 1;
        }
        c.version += 1;
        let bbox = c.bbox;
        let root = c.root;
        let size = bbox.size();
        if !accepted
            && config.position.free_space.local_refinement
            && c.failures >= config.position.free_space.candidates_per_location
            && size.x.min(size.y).min(size.z) * 0.5 >= config.position.free_space.min_cell_size
            && can_grow
        {
            let clearance = c.clearance;
            c.active = false;
            let mid = midpoint(bbox);
            for bits in 0..8 {
                let lo = Vec3::new(
                    if bits & 1 == 0 { bbox.min.x } else { mid.x },
                    if bits & 2 == 0 { bbox.min.y } else { mid.y },
                    if bits & 4 == 0 { bbox.min.z } else { mid.z },
                );
                let hi = Vec3::new(
                    if bits & 1 == 0 { mid.x } else { bbox.max.x },
                    if bits & 2 == 0 { mid.y } else { bbox.max.y },
                    if bits & 4 == 0 { mid.z } else { bbox.max.z },
                );
                // Parent distance plus centre displacement is a ranking bound,
                // not a claim that the child is geometrically free.
                self.add(
                    BoundingBox { min: lo, max: hi },
                    root,
                    clearance + vec_norm(midpoint(BoundingBox { min: lo, max: hi }).sub(mid)),
                );
            }
            self.stats.refinements += 1;
        } else {
            self.push(id);
        }
        if self.heap.len() > self.cells.len().saturating_mul(2) {
            self.rebuild();
        }
    }

    // AI-FUNC-SUMMARY: Invalidate only cells near new real material. Far scores cannot change within the capped query radius.
    pub fn inserted(&mut self, config: &ResolvedPlacement, bbox: BoundingBox) {
        let affected = bbox.expanded(2.0 * self.coarse_size + config.gap_particle_particle);
        let to_cell = |v: f64, axis: usize| -> usize {
            let origin = [
                config.domain.min.x,
                config.domain.min.y,
                config.domain.min.z,
            ][axis];
            (((v - origin) / self.coarse_size).floor().max(0.0) as usize)
                .min(self.dimensions[axis] - 1)
        };
        let lo = [
            to_cell(affected.min.x, 0),
            to_cell(affected.min.y, 1),
            to_cell(affected.min.z, 2),
        ];
        let hi = [
            to_cell(affected.max.x, 0),
            to_cell(affected.max.y, 1),
            to_cell(affected.max.z, 2),
        ];
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let root = (x * self.dimensions[1] + y) * self.dimensions[2] + z;
                    let ids = self.roots[root].clone();
                    for id in ids {
                        let c = &mut self.cells[id];
                        if c.active && !c.dirty {
                            c.dirty = true;
                            c.version += 1;
                            self.push(id);
                        }
                    }
                }
            }
        }
        if self.heap.len() > self.cells.len().saturating_mul(2) {
            self.rebuild();
        }
    }

    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"cells":self.cells.len(),"active_cells":self.cells.iter().filter(|c| c.active).count(),
            "estimated_index_bytes":self.cells.len()*512,"cell_capacity":self.capacity,"stats":self.stats})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ResolvedPlacement {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "placement:\n  seed: 11\n  frame: {unit: um}\n  domain: {min: [0,0,0], max: [10,10,10]}\n  shapes: {files: [source.stl]}\n  size:\n    distribution: {kind: lognormal, median: 1, sigma_log: 0.1, min: 0.8, max: 1.2}\n    classes: {kind: equal_width, count: 2}\n  position: {mode: free_space_guided, free_space: {coarse_cell_size: 2, min_cell_size: 0.5, max_cells: 1000, index_memory_mb: 1, candidates_per_location: 1, exploration_fraction: 0}}\n  boundary: {mode: strict}\n  target: {volume_fraction: 0.01}\n  outputs: {dir: out}\n").unwrap();
        match crate::config::load_pack_document(&path).unwrap() {
            crate::config::PackDocument::Placement(p) => p.validate(&path).unwrap(),
            _ => panic!("placement"),
        }
    }

    #[test]
    fn lazy_scoring_can_stop_and_restore_without_changing_next_proposal() {
        let mut config = fixture();
        config.domain.max = Vec3::new(12.0, 12.0, 12.0);
        let grid = SpatialGrid::new(config.domain, 2.0);
        let mut uninterrupted = Index::new(&config).unwrap();
        let mut rng = seeded_rng(11);
        let point = uninterrupted
            .propose(
                &config,
                &[],
                &grid,
                None,
                &mut rng,
                config.domain,
                1.0,
                || false,
            )
            .unwrap();
        let mut stopped = Index::new(&config).unwrap();
        let mut rng2 = seeded_rng(11);
        let before = rng2.get_word_pos();
        let mut polls = 0;
        assert!(stopped
            .propose(
                &config,
                &[],
                &grid,
                None,
                &mut rng2,
                config.domain,
                1.0,
                || {
                    polls += 1;
                    polls > 1
                }
            )
            .is_none());
        assert_eq!(stopped.stats.score_queries, 64);
        rng2.set_word_pos(before); // Caller rewinds uncommitted source/orientation/position variates.
        let mut restored: Index =
            serde_json::from_slice(&serde_json::to_vec(&stopped).unwrap()).unwrap();
        restored.rebuild();
        let next = restored
            .propose(
                &config,
                &[],
                &grid,
                None,
                &mut rng2,
                config.domain,
                1.0,
                || false,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(point.0).unwrap(),
            serde_json::to_value(next.0).unwrap()
        );
        assert_eq!(point.1, next.1);
        assert_eq!(rng.get_word_pos(), rng2.get_word_pos());
        assert_eq!(
            serde_json::to_value(&uninterrupted).unwrap(),
            serde_json::to_value(&restored).unwrap()
        );
    }

    #[test]
    fn refinement_respects_capacity_without_excluding_uncertain_cells() {
        let mut config = fixture();
        config.position.free_space.max_cells = 133;
        let mut index = Index::new(&config).unwrap();
        index.feedback(&config, Some(0), false);
        assert_eq!(index.cells.len(), 133);
        assert!(!index.cells[0].active);
        index.feedback(&config, Some(1), false);
        assert_eq!(index.cells.len(), 133);
        assert!(index.cells[1].active);
        assert!(index.cells[125..].iter().all(|c| c.active && c.dirty));
        config.position.free_space.max_cells = 10000;
        config.position.free_space.index_memory_mb = 1;
        assert_eq!(Index::new(&config).unwrap().capacity, 1024 * 1024 / 512);
    }
    #[test]
    fn an_empty_region_inside_a_particle_bbox_has_positive_real_clearance() {
        let mut config = fixture();
        config.gap_particle_particle = 0.05;
        let centre = Vec3::new(5.0, 5.0, 5.0);
        let mesh = crate::geometry::icosphere_mesh(centre, 2.0, 1);
        let bbox = crate::geometry::mesh_bbox(&mesh).unwrap();
        let point = Vec3::new(6.6, 6.6, 5.0);
        assert!(bbox.contains_point(point));
        let placed = vec![PlacedParticle {
            acceptance_index: 0,
            source_index: 0,
            shell_index: 0,
            scale: 1.0,
            rotation: UnitQuat {
                w: 1.0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            translation: centre,
            equivalent_diameter: 4.0,
            reach: 2.0,
            size_class: 0,
            volume_full: 1.0,
            volume_in_domain: 1.0,
            void_overlap_volume: 0.0,
            clipped_faces: vec![],
            bbox,
            shape: crate::geometry::to_parry_trimesh(&mesh),
            mesh,
            geometry: None,
            triangle_range: (0, 80),
        }];
        let mut grid = SpatialGrid::new(config.domain, 2.0);
        grid.insert(0, bbox);
        assert!(Index::score(&config, &placed, &grid, None, point) > 0.1);
        assert!(Index::score(&config, &placed, &grid, None, centre) < 0.0);
    }
}
