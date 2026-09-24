use super::bbox::mesh_bbox;
use super::s2::{ray_intersects_triangle, RAY_DIR_GPU};
use crate::types::{BoundingBox, Mesh, Vec3};

// AI-FUNC-SUMMARY: Reusable per-query hit storage and narrow-phase work counter; never shared between concurrent queries.
#[derive(Default)]
pub struct MeshQueryScratch {
    hits: Vec<f64>,
    pub triangle_tests: usize,
}

struct Node {
    bbox: BoundingBox,
    start: usize,
    end: usize,
    escape: usize,
}

// AI-FUNC-SUMMARY: Immutable prepared ray-query view over a borrowed mesh, with cached bbox and flat median-split BVH; rebuild after any mesh mutation.
pub struct PreparedMeshQuery<'a> {
    mesh: &'a Mesh,
    bbox: Option<BoundingBox>,
    ids: Vec<usize>,
    nodes: Vec<Node>,
}

impl<'a> PreparedMeshQuery<'a> {
    // AI-FUNC-SUMMARY: Cache mesh bounds and build a flat BVH for meshes above 32 faces; small or nonfinite geometry retains direct traversal without repeated bbox scans.
    pub fn new(mesh: &'a Mesh) -> Self {
        let bbox = mesh_bbox(mesh);
        let mut query = Self {
            mesh,
            bbox,
            ids: (0..mesh.faces.len()).collect(),
            nodes: Vec::new(),
        };
        if mesh.faces.len() > 32
            && mesh
                .vertices
                .iter()
                .all(|v| [v.x, v.y, v.z].iter().all(|c| c.is_finite()))
        {
            let boxes: Vec<_> = mesh
                .faces
                .iter()
                .map(|f| {
                    let [a, b, c] = [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]];
                    BoundingBox {
                        min: Vec3::new(
                            a.x.min(b.x).min(c.x),
                            a.y.min(b.y).min(c.y),
                            a.z.min(b.z).min(c.z),
                        ),
                        max: Vec3::new(
                            a.x.max(b.x).max(c.x),
                            a.y.max(b.y).max(c.y),
                            a.z.max(b.z).max(c.z),
                        ),
                    }
                })
                .collect();
            build_nodes(&mut query.ids, 0, &boxes, &mut query.nodes);
        }
        query
    }

    // AI-FUNC-SUMMARY: Return the cached whole-mesh bounds, including unused vertices exactly as the standalone reference does.
    pub fn bbox(&self) -> Option<BoundingBox> {
        self.bbox
    }

    // AI-FUNC-SUMMARY: Test parity using cached broad-phase bounds and the unchanged CPU triangle/anchored deduplication rules; clear and reuse caller-owned scratch and count actual triangle tests.
    pub fn contains_point(&self, point: Vec3, scratch: &mut MeshQueryScratch) -> bool {
        scratch.hits.clear();
        scratch.triangle_tests = 0;
        let Some(bb) = self.bbox else {
            return false;
        };
        let eps = 1e-9;
        if point.x < bb.min.x - eps
            || point.x > bb.max.x + eps
            || point.y < bb.min.y - eps
            || point.y > bb.max.y + eps
            || point.z < bb.min.z - eps
            || point.z > bb.max.z + eps
        {
            return false;
        }
        let dir = Vec3::new(RAY_DIR_GPU.0, RAY_DIR_GPU.1, RAY_DIR_GPU.2);
        if self.nodes.is_empty() {
            self.intersect_range(0, self.ids.len(), point, dir, scratch);
        } else {
            let mut i = 0;
            while i < self.nodes.len() {
                let node = &self.nodes[i];
                if !ray_reaches_box(point, dir, node.bbox) {
                    i = node.escape;
                    continue;
                }
                self.intersect_range(node.start, node.end, point, dir, scratch);
                i += 1;
            }
        }
        scratch
            .hits
            .sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        let mut unique = 0usize;
        let mut last = f64::NEG_INFINITY;
        for &t in &scratch.hits {
            if (t - last).abs() > 1e-8 {
                unique += 1;
                last = t;
            }
        }
        unique % 2 == 1
    }

    // AI-FUNC-SUMMARY: Apply the unchanged narrow-phase predicate to one leaf or direct range, appending distances to reusable scratch.
    fn intersect_range(
        &self,
        start: usize,
        end: usize,
        point: Vec3,
        dir: Vec3,
        scratch: &mut MeshQueryScratch,
    ) {
        for &id in &self.ids[start..end] {
            let f = &self.mesh.faces[id];
            scratch.triangle_tests += 1;
            if let Some(t) = ray_intersects_triangle(
                point,
                dir,
                self.mesh.vertices[f.a],
                self.mesh.vertices[f.b],
                self.mesh.vertices[f.c],
            ) {
                scratch.hits.push(t);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Build preorder BVH nodes with escape indices and leaves of at most eight faces; median partition preserves a deterministic tie break without duplicating geometry.
fn build_nodes(ids: &mut [usize], offset: usize, boxes: &[BoundingBox], nodes: &mut Vec<Node>) {
    let mut bbox = boxes[ids[0]];
    for &id in &ids[1..] {
        let b = boxes[id];
        bbox.min = Vec3::new(
            bbox.min.x.min(b.min.x),
            bbox.min.y.min(b.min.y),
            bbox.min.z.min(b.min.z),
        );
        bbox.max = Vec3::new(
            bbox.max.x.max(b.max.x),
            bbox.max.y.max(b.max.y),
            bbox.max.z.max(b.max.z),
        );
    }
    let i = nodes.len();
    nodes.push(Node {
        bbox,
        start: offset,
        end: offset + ids.len(),
        escape: 0,
    });
    if ids.len() > 8 {
        let size = bbox.size();
        let axis = if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        };
        let middle = ids.len() / 2;
        ids.select_nth_unstable_by(middle, |&a, &b| {
            let amin = [boxes[a].min.x, boxes[a].min.y, boxes[a].min.z][axis];
            let amax = [boxes[a].max.x, boxes[a].max.y, boxes[a].max.z][axis];
            let bmin = [boxes[b].min.x, boxes[b].min.y, boxes[b].min.z][axis];
            let bmax = [boxes[b].max.x, boxes[b].max.y, boxes[b].max.z][axis];
            (amin * 0.5 + amax * 0.5)
                .total_cmp(&(bmin * 0.5 + bmax * 0.5))
                .then(a.cmp(&b))
        });
        let (left, right) = ids.split_at_mut(middle);
        build_nodes(left, offset, boxes, nodes);
        build_nodes(right, offset + middle, boxes, nodes);
        nodes[i].start = 0;
        nodes[i].end = 0;
    }
    nodes[i].escape = nodes.len();
}

// AI-FUNC-SUMMARY: Conservative positive-ray slab prefilter with padding for the CPU barycentric tolerance and coordinate roundoff; uncertain/nonfinite arithmetic keeps the node.
fn ray_reaches_box(origin: Vec3, dir: Vec3, bbox: BoundingBox) -> bool {
    let mut near = 0.0f64;
    let mut far = f64::INFINITY;
    for (o, d, lo, hi) in [
        (origin.x, dir.x, bbox.min.x, bbox.max.x),
        (origin.y, dir.y, bbox.min.y, bbox.max.y),
        (origin.z, dir.z, bbox.min.z, bbox.max.z),
    ] {
        let pad = 1e-9
            + (hi - lo).abs() * 4e-10
            + lo.abs().max(hi.abs()).max(o.abs()) * 64.0 * f64::EPSILON;
        let a = (lo - pad - o) / d;
        let b = (hi + pad - o) / d;
        if !a.is_finite() || !b.is_finite() {
            return true;
        }
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    far >= near
}
