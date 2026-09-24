use crate::types::{BoundingBox, Vec3};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct SpatialQueryScratch {
    pub neighbors: Vec<usize>,
    seen: HashSet<usize>,
}

pub struct SpatialGrid {
    inv_cell: f64,
    nx: usize,
    ny: usize,
    nz: usize,
    origin: Vec3,
    cells: Vec<Vec<usize>>,
    memberships: HashMap<usize, Vec<usize>>,
}

impl SpatialGrid {
    // AI-FUNC-SUMMARY: Create a new SpatialGrid partitioning the given box bounds into cubic cells; returns SpatialGrid; side effects: None.
    pub fn new(box_bounds: BoundingBox, cell_size: f64) -> Self {
        let size = box_bounds.size();
        let nx = ((size.x / cell_size).ceil() as usize).max(1);
        let ny = ((size.y / cell_size).ceil() as usize).max(1);
        let nz = ((size.z / cell_size).ceil() as usize).max(1);

        SpatialGrid {
            inv_cell: 1.0 / cell_size,
            nx,
            ny,
            nz,
            origin: box_bounds.min,
            cells: vec![Vec::new(); nx * ny * nz],
            memberships: HashMap::new(),
        }
    }

    // AI-FUNC-SUMMARY: Insert an item index into all grid cells overlapped by its bounding box; mutates grid cells; side effects: None.
    pub fn insert(&mut self, idx: usize, bbox: BoundingBox) {
        let (x0, y0, z0) = self.point_to_cell_clamped(bbox.min);
        let (x1, y1, z1) = self.point_to_cell_clamped(bbox.max);
        let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let (z0, z1) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };

        for cx in x0..=x1 {
            for cy in y0..=y1 {
                for cz in z0..=z1 {
                    let cell_idx = cx * self.ny * self.nz + cy * self.nz + cz;
                    self.cells[cell_idx].push(idx);
                    self.memberships.entry(idx).or_default().push(cell_idx);
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Remove every insertion of an item using reverse cell membership; preserves the order of remaining bucket entries and returns whether the item existed.
    pub fn remove(&mut self, idx: usize) -> bool {
        let Some(mut cells) = self.memberships.remove(&idx) else {
            return false;
        };
        cells.sort_unstable();
        cells.dedup();
        for cell in cells {
            self.cells[cell].retain(|&item| item != idx);
        }
        true
    }

    // AI-FUNC-SUMMARY: Replace an item's previous cell memberships with a new bbox, or remove it for None; unrelated cells remain unchanged.
    pub fn update(&mut self, idx: usize, bbox: Option<BoundingBox>) {
        self.remove(idx);
        if let Some(bbox) = bbox {
            self.insert(idx, bbox);
        }
    }

    // AI-FUNC-SUMMARY: Build a SpatialGrid by inserting all (index, bbox) pairs; returns populated SpatialGrid; side effects: None.
    pub fn build(bboxes: &[(usize, BoundingBox)], box_bounds: BoundingBox, cell_size: f64) -> Self {
        let mut grid = Self::new(box_bounds, cell_size);
        for &(idx, bbox) in bboxes {
            grid.insert(idx, bbox);
        }
        grid
    }

    // AI-FUNC-SUMMARY: Find all item indices in grid cells overlapping the given bbox, excluding the specified index; returns Vec<usize>; side effects: None.
    pub fn query_neighbors(&self, bbox: BoundingBox, exclude: usize) -> Vec<usize> {
        self.query_neighbors_with_margin(bbox, 0.0, exclude)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Find all item indices in grid cells overlapping the given bbox plus a margin, excluding the specified index.
    // Inputs: query bbox, margin distance (adds extra cell padding), index to exclude.
    // Returns: Vec<usize> of neighbor indices in first-encounter order (deduplicated).
    // Side effects: None.
    // Notes: Small results use linear deduplication; as soon as the query reaches 256 unique IDs a membership set bounds duplicate lookup cost. The set is never iterated.
    pub fn query_neighbors_with_margin(
        &self,
        bbox: BoundingBox,
        margin: f64,
        exclude: usize,
    ) -> Vec<usize> {
        let mut scratch = SpatialQueryScratch::default();
        self.query_into(bbox, margin, exclude, &mut scratch);
        scratch.neighbors
    }

    // AI-FUNC-SUMMARY: Fill caller-owned neighbors with first-encounter candidates, retaining vector/hash capacity across queries; previous contents are cleared.
    pub fn query_into(
        &self,
        bbox: BoundingBox,
        margin: f64,
        exclude: usize,
        scratch: &mut SpatialQueryScratch,
    ) {
        scratch.neighbors.clear();
        scratch.seen.clear();
        let pad = ((margin.max(0.0) * self.inv_cell).ceil() as usize).saturating_add(1);
        let (x0, y0, z0) = self.point_to_cell_clamped(bbox.min);
        let (x1, y1, z1) = self.point_to_cell_clamped(bbox.max);
        let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let (z0, z1) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };

        let neighbors = &mut scratch.neighbors;
        let seen = &mut scratch.seen;
        let mut hashed = false;
        for cx in x0.saturating_sub(pad)..=x1.saturating_add(pad).min(self.nx - 1) {
            for cy in y0.saturating_sub(pad)..=y1.saturating_add(pad).min(self.ny - 1) {
                for cz in z0.saturating_sub(pad)..=z1.saturating_add(pad).min(self.nz - 1) {
                    let cell_idx = cx * self.ny * self.nz + cy * self.nz + cz;
                    for &idx in &self.cells[cell_idx] {
                        if idx == exclude {
                            continue;
                        }
                        if hashed {
                            if seen.insert(idx) { neighbors.push(idx); }
                        } else if !neighbors.contains(&idx) {
                            neighbors.push(idx);
                            // Switch within the bucket: a single dense bucket must not stay quadratic.
                            if neighbors.len() == 256 {
                                seen.extend(neighbors.iter().copied());
                                hashed = true;
                            }
                        }
                    }

                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Map a point to grid cell coordinates, clamping to valid range; returns (cx, cy, cz) clamped to grid bounds; side effects: None.
    fn point_to_cell_clamped(&self, p: Vec3) -> (usize, usize, usize) {
        let (cx, cy, cz) = self.point_to_cell(p);
        (
            cx.min(self.nx - 1),
            cy.min(self.ny - 1),
            cz.min(self.nz - 1),
        )
    }

    // AI-FUNC-SUMMARY: Map a point to grid cell coordinates (unclamped, may exceed grid bounds); returns (cx, cy, cz); side effects: None.
    fn point_to_cell(&self, p: Vec3) -> (usize, usize, usize) {
        let cx = (((p.x - self.origin.x) * self.inv_cell).floor() as isize).max(0) as usize;
        let cy = (((p.y - self.origin.y) * self.inv_cell).floor() as isize).max(0) as usize;
        let cz = (((p.z - self.origin.z) * self.inv_cell).floor() as isize).max(0) as usize;
        (cx, cy, cz)
    }
}

// AI-FUNC-SUMMARY: Estimate a reasonable SpatialGrid cell size as the maximum extent of any bounding box; returns f64 (min 1.0); side effects: None.
pub fn estimate_cell_size(bboxes: &[BoundingBox]) -> f64 {
    if bboxes.is_empty() {
        return 1.0;
    }

    let mut max_extent = 0.0f64;
    for bbox in bboxes {
        let size = bbox.size();
        max_extent = max_extent.max(size.x).max(size.y).max(size.z);
    }
    max_extent.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    // AI-FUNC-SUMMARY: Original linear-dedup grid query retained as an order-sensitive oracle and benchmark baseline; no side effects.
    fn linear_query(
        grid: &SpatialGrid,
        bbox: BoundingBox,
        margin: f64,
        exclude: usize,
    ) -> Vec<usize> {
        let pad = (margin.max(0.0) * grid.inv_cell).ceil() as usize + 1;
        let (x0, y0, z0) = grid.point_to_cell_clamped(bbox.min);
        let (x1, y1, z1) = grid.point_to_cell_clamped(bbox.max);
        let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let (z0, z1) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };

        let mut neighbors = Vec::new();
        for cx in x0.saturating_sub(pad)..=x1.saturating_add(pad).min(grid.nx - 1) {
            for cy in y0.saturating_sub(pad)..=y1.saturating_add(pad).min(grid.ny - 1) {
                for cz in z0.saturating_sub(pad)..=z1.saturating_add(pad).min(grid.nz - 1) {
                    let cell_idx = cx * grid.ny * grid.nz + cy * grid.nz + cz;
                    for &idx in &grid.cells[cell_idx] {
                        if idx != exclude && !neighbors.contains(&idx) {
                            neighbors.push(idx);
                        }
                    }
                }
            }
        }
        neighbors
    }

    // AI-FUNC-SUMMARY: Build a cubic bbox for spatial-query fixtures; no side effects.
    fn bounds(lo: f64, hi: f64) -> BoundingBox {
        BoundingBox {
            min: Vec3::new(lo, lo, lo),
            max: Vec3::new(hi, hi, hi),
        }
    }

    // AI-FUNC-SUMMARY: Compare exact neighbor order against the original scan for sparse IDs, repeated inserts, reversed and clamped boxes and margins; no side effects.
    #[test]
    fn query_preserves_order_and_exclusions() {
        let mut grid = SpatialGrid::new(bounds(0.0, 8.0), 1.0);
        for i in 0..600 {
            let lo = ((i * 37) % 103) as f64 / 10.0 - 2.0;
            grid.insert(usize::MAX - i, bounds(lo, lo + (i % 7) as f64));
        }
        grid.insert(usize::MAX - 20, bounds(0.0, 8.0));
        for bbox in [
            bounds(-10.0, -1.0),
            bounds(7.0, 12.0),
            bounds(1.0, 2.0),
            bounds(5.0, 2.0),
            bounds(0.0, 8.0),
        ] {
            for margin in [-1.0, 0.0, 0.25, 2.0, 20.0] {
                for exclude in [0, usize::MAX, usize::MAX - 20] {
                    assert_eq!(
                        grid.query_neighbors_with_margin(bbox, margin, exclude),
                        linear_query(&grid, bbox, margin, exclude)
                    );
                }
            }
        }
        let empty = SpatialGrid::new(bounds(0.0, 8.0), 1.0);
        assert!(empty.query_neighbors(bounds(0.0, 8.0), 0).is_empty());
    }

    // AI-FUNC-SUMMARY: Compare incremental moves/removals/reinsertions against complete rebuilds, including repeated IDs and outside-domain boxes.
    #[test]
    fn updates_match_rebuilt_candidate_sets() {
        let domain = bounds(0.0, 12.0);
        let mut boxes: Vec<_> = (0..40)
            .map(|i| (i * 1009, bounds((i % 10) as f64, (i % 10) as f64 + 1.5)))
            .collect();
        let mut grid = SpatialGrid::build(&boxes, domain, 1.0);
        for step in 0..100 {
            let index = step % boxes.len();
            let lo = ((step * 37) % 180) as f64 / 10.0 - 3.0;
            boxes[index].1 = bounds(lo, lo + 2.0);
            grid.update(boxes[index].0, Some(boxes[index].1));
            let rebuilt = SpatialGrid::build(&boxes, domain, 1.0);
            for q in [bounds(-4.0, -2.0), bounds(3.0, 5.0), bounds(10.0, 15.0)] {
                let mut actual = grid.query_neighbors_with_margin(q, 0.2, boxes[index].0);
                let mut expected = rebuilt.query_neighbors_with_margin(q, 0.2, boxes[index].0);
                actual.sort_unstable();
                expected.sort_unstable();
                assert_eq!(actual, expected);
            }
        }
        grid.insert(usize::MAX, domain);
        grid.insert(usize::MAX, bounds(2.0, 3.0));
        assert!(grid.remove(usize::MAX));
        assert!(!grid.remove(usize::MAX));
        assert!(!grid.query_neighbors(domain, 0).contains(&usize::MAX));
        grid.update(boxes[0].0, None);
        assert!(!grid
            .query_neighbors(domain, usize::MAX)
            .contains(&boxes[0].0));
    }

    // AI-FUNC-SUMMARY: Measure incremental membership replacement versus full grid rebuild with identical accepted moves and fixed geometry; prints raw release timings.
    #[test]
    #[ignore = "release performance measurement"]
    fn update_benchmark() {
        for count in [16, 256, 4096] {
            let domain = bounds(0.0, 40.0);
            let boxes: Vec<_> = (0..count)
                .map(|i| {
                    let p = Vec3::new(
                        (i % 32) as f64,
                        ((i / 32) % 32) as f64,
                        ((i / 1024) % 32) as f64,
                    );
                    (
                        i,
                        BoundingBox {
                            min: p,
                            max: p.add(Vec3::new(0.8, 0.8, 0.8)),
                        },
                    )
                })
                .collect();
            for trial in 0..6 {
                let mut times = [0.0; 2];
                for mode in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let mut moved = boxes.clone();
                    let mut grid = SpatialGrid::build(&moved, domain, 2.0);
                    let start = Instant::now();
                    for step in 0..200 {
                        let idx = step % count;
                        let shift = Vec3::new(0.01, 0.02, 0.03);
                        moved[idx].1.min = moved[idx].1.min.add(shift);
                        moved[idx].1.max = moved[idx].1.max.add(shift);
                        if mode == 0 {
                            grid = SpatialGrid::build(&moved, domain, 2.0);
                        } else {
                            grid.update(idx, Some(moved[idx].1));
                        }
                        black_box(&grid);
                    }
                    times[mode] = start.elapsed().as_secs_f64();
                }
                println!(
                    "grid_update count={count} trial={trial} moves=200 rebuild_s={} update_s={}",
                    times[0], times[1]
                );
            }
        }
    }

    // AI-FUNC-SUMMARY: Measure five alternating release samples of original and optimized complete queries at small/medium/large candidate counts; prints raw timings, ignored normally.
    #[test]
    #[ignore = "release performance measurement"]
    fn query_benchmark() {
        for (count, repeats) in [(8, 10000), (64, 1000), (256, 100), (1024, 20)] {
            let domain = bounds(0.0, 4.0);
            let grid = SpatialGrid::build(
                &(0..count).map(|i| (i, domain)).collect::<Vec<_>>(),
                domain,
                1.0,
            );
            assert_eq!(
                grid.query_neighbors(domain, usize::MAX),
                linear_query(&grid, domain, 0.0, usize::MAX)
            );
            for _ in 0..10 {
                black_box(grid.query_neighbors(domain, usize::MAX));
                black_box(linear_query(&grid, domain, 0.0, usize::MAX));
            }
            for sample in 0..5 {
                let mut times = [0.0; 2];
                for which in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let start = Instant::now();
                    for _ in 0..repeats {
                        black_box(if which == 0 {
                            linear_query(black_box(&grid), domain, 0.0, usize::MAX)
                        } else {
                            grid.query_neighbors(black_box(domain), usize::MAX)
                        });
                    }
                    times[which] = start.elapsed().as_secs_f64() * 1e6 / repeats as f64;
                }
                println!("query count={count} references={} sample={sample} repeats={repeats} linear_us={:.3} optimized_us={:.3}", count * 64, times[0], times[1]);
            }
        }
    }
    // AI-FUNC-SUMMARY: Verify hash promotion inside a single dense bucket, retained scratch reuse, exclusions and duplicates.
    #[test]
    fn dense_bucket_promotes_before_finishing_bucket() {
        let domain = bounds(0.0, 1.0);
        let mut grid = SpatialGrid::new(domain, 2.0);
        for i in 0..4096 { grid.insert(usize::MAX - i, domain); }
        for i in 0..300 { grid.insert(usize::MAX - i, domain); }
        let mut scratch = SpatialQueryScratch::default();
        for exclude in [usize::MAX, usize::MAX - 255, 0] {
            grid.query_into(domain, 0.0, exclude, &mut scratch);
            assert_eq!(scratch.neighbors, linear_query(&grid, domain, 0.0, exclude));
            assert_eq!(scratch.seen.len(), scratch.neighbors.len());
        }
        grid.query_into(domain, 0.0, usize::MAX, &mut scratch);
        assert_eq!(scratch.neighbors[0], usize::MAX - 1);
    }

    // AI-FUNC-SUMMARY: Compare complete one-bucket queries with linear oracle at increasing density; record raw release samples.
    #[test]
    #[ignore = "release performance measurement"]
    fn dense_bucket_benchmark() {
        let domain = bounds(0.0, 1.0);
        for count in [32, 256, 4096, 16384] {
            let grid = SpatialGrid::build(&(0..count).map(|i| (i, domain)).collect::<Vec<_>>(), domain, 2.0);
            for sample in 0..6 {
                let mut times = [0.0; 2];
                for which in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let start = Instant::now();
                    let result = if which == 0 { linear_query(&grid, domain, 0.0, usize::MAX) } else { grid.query_neighbors(domain, usize::MAX) };
                    times[which] = start.elapsed().as_secs_f64();
                    assert_eq!(result, (0..count).collect::<Vec<_>>());
                }
                println!("dense_bucket count={count} sample={sample} linear_seconds={:.9} query_seconds={:.9}", times[0], times[1]);
            }
        }
    }

}
