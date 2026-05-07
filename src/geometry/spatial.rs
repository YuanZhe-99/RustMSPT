use crate::types::{BoundingBox, Vec3};

pub struct SpatialGrid {
    inv_cell: f64,
    nx: usize,
    ny: usize,
    nz: usize,
    origin: Vec3,
    cells: Vec<Vec<usize>>,
}

impl SpatialGrid {
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
        }
    }

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
                }
            }
        }
    }

    pub fn build(bboxes: &[(usize, BoundingBox)], box_bounds: BoundingBox, cell_size: f64) -> Self {
        let mut grid = Self::new(box_bounds, cell_size);
        for &(idx, bbox) in bboxes {
            grid.insert(idx, bbox);
        }
        grid
    }

    pub fn query_neighbors(&self, bbox: BoundingBox, exclude: usize) -> Vec<usize> {
        self.query_neighbors_with_margin(bbox, 0.0, exclude)
    }

    pub fn query_neighbors_with_margin(&self, bbox: BoundingBox, margin: f64, exclude: usize) -> Vec<usize> {
        let pad = (margin.max(0.0) * self.inv_cell).ceil() as usize + 1;
        let (x0, y0, z0) = self.point_to_cell_clamped(bbox.min);
        let (x1, y1, z1) = self.point_to_cell_clamped(bbox.max);
        let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let (z0, z1) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };

        let mut neighbors = Vec::new();
        for cx in x0.saturating_sub(pad)..=x1.saturating_add(pad).min(self.nx - 1) {
            for cy in y0.saturating_sub(pad)..=y1.saturating_add(pad).min(self.ny - 1) {
                for cz in z0.saturating_sub(pad)..=z1.saturating_add(pad).min(self.nz - 1) {
                    let cell_idx = cx * self.ny * self.nz + cy * self.nz + cz;
                    for &idx in &self.cells[cell_idx] {
                        if idx != exclude && !neighbors.contains(&idx) {
                            neighbors.push(idx);
                        }
                    }
                }
            }
        }
        neighbors
    }

    fn point_to_cell_clamped(&self, p: Vec3) -> (usize, usize, usize) {
        let (cx, cy, cz) = self.point_to_cell(p);
        (cx.min(self.nx - 1), cy.min(self.ny - 1), cz.min(self.nz - 1))
    }

    fn point_to_cell(&self, p: Vec3) -> (usize, usize, usize) {
        let cx = (((p.x - self.origin.x) * self.inv_cell).floor() as isize).max(0) as usize;
        let cy = (((p.y - self.origin.y) * self.inv_cell).floor() as isize).max(0) as usize;
        let cz = (((p.z - self.origin.z) * self.inv_cell).floor() as isize).max(0) as usize;
        (cx, cy, cz)
    }
}

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
