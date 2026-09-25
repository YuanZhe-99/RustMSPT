use crate::types::{BoundingBox, Mesh};

// AI-FUNC-SUMMARY:
// Purpose: Compute the axis-aligned bounding box of a mesh.
// Inputs: mesh reference.
// Returns: Some(BoundingBox) enclosing all vertices, or None for empty mesh.
// Side effects: None.
pub fn mesh_bbox(mesh: &Mesh) -> Option<BoundingBox> {
    if mesh.vertices.is_empty() {
        return None;
    }

    let mut min = mesh.vertices[0];
    let mut max = mesh.vertices[0];
    for v in &mesh.vertices {
        min.x = min.x.min(v.x);
        min.y = min.y.min(v.y);
        min.z = min.z.min(v.z);
        max.x = max.x.max(v.x);
        max.y = max.y.max(v.y);
        max.z = max.z.max(v.z);
    }

    Some(BoundingBox { min, max })
}

// AI-FUNC-SUMMARY: Check whether two bounding boxes overlap on all three axes (strict overlap, not just touching); returns bool; side effects: None.
pub fn bbox_overlaps(a: BoundingBox, b: BoundingBox) -> bool {
    a.min.x < b.max.x
        && a.max.x > b.min.x
        && a.min.y < b.max.y
        && a.max.y > b.min.y
        && a.min.z < b.max.z
        && a.max.z > b.min.z
}

// AI-FUNC-SUMMARY: Compute the minimum Euclidean distance between two bounding boxes; returns 0.0 when they overlap; side effects: None.
pub fn bbox_distance(a: BoundingBox, b: BoundingBox) -> f64 {
    let dx = if a.max.x < b.min.x {
        b.min.x - a.max.x
    } else if b.max.x < a.min.x {
        a.min.x - b.max.x
    } else {
        0.0
    };

    let dy = if a.max.y < b.min.y {
        b.min.y - a.max.y
    } else if b.max.y < a.min.y {
        a.min.y - b.max.y
    } else {
        0.0
    };

    let dz = if a.max.z < b.min.z {
        b.min.z - a.max.z
    } else if b.max.z < a.min.z {
        a.min.z - b.max.z
    } else {
        0.0
    };

    (dx * dx + dy * dy + dz * dz).sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: Validate a mesh against boundary constraint mode for placement in a periodic or bounded box.
// Inputs: mesh, box_bounds, mode (1=fully inside only, 2/3=allow periodic crossing), d1 (min boundary distance for interior particles), d2 (min cross-boundary depth).
// Returns: true if the mesh satisfies the boundary constraints.
// Side effects: None.
// Notes: Mode 1 rejects any particle not fully inside. Modes 2/3 allow periodic wrapping with depth constraints.
pub fn check_boundary_constraints_mode(
    mesh: &Mesh,
    box_bounds: BoundingBox,
    mode: u8,
    d1: f64,
    d2: f64,
) -> bool {
    let Some(bounds) = mesh_bbox(mesh) else {
        return false;
    };

    let local_min = bounds.min.sub(box_bounds.min);
    let local_max = bounds.max.sub(box_bounds.min);
    let size = box_bounds.size();

    let is_fully_inside = local_min.x >= 0.0
        && local_min.y >= 0.0
        && local_min.z >= 0.0
        && local_max.x <= size.x
        && local_max.y <= size.y
        && local_max.z <= size.z;

    if is_fully_inside {
        if local_min.x < d1
            || local_min.y < d1
            || local_min.z < d1
            || local_max.x > (size.x - d1)
            || local_max.y > (size.y - d1)
            || local_max.z > (size.z - d1)
        {
            return false;
        }
        return true;
    }

    if mode == 1 {
        return false;
    }

    let min_arr = [local_min.x, local_min.y, local_min.z];
    let max_arr = [local_max.x, local_max.y, local_max.z];
    let size_arr = [size.x, size.y, size.z];

    for i in 0..3 {
        if min_arr[i] < 0.0
            && (min_arr[i].abs() < d2 || max_arr[i] < d2) {
                return false;
            }
        if max_arr[i] > size_arr[i]
            && ((size_arr[i] - min_arr[i]) < d2 || (max_arr[i] - size_arr[i]) < d2) {
                return false;
            }
        if min_arr[i] >= 0.0 && max_arr[i] <= size_arr[i]
            && (min_arr[i] < d1 || max_arr[i] > (size_arr[i] - d1)) {
                return false;
            }
    }

    true
}
